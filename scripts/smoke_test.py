#!/usr/bin/env python3
"""Run an isolated HTTP/TCP/AMQP/SQLite scenario; requires a local RabbitMQ broker.

Build first with `cargo build --workspace --locked`. No third-party Python packages.
Temporary databases, subprocesses, and this run's queue are cleaned up on exit.
"""
import base64
import json
import os
from pathlib import Path
import socket
import sqlite3
import struct
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]


def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for(check, processes, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for process in processes:
            if process.poll() is not None:
                raise RuntimeError(f"service exited with code {process.returncode}")
        try:
            result = check()
            if result:
                return result
        except (OSError, sqlite3.Error):
            pass
        time.sleep(0.05)
    raise TimeoutError("condition did not become true")


def port_ready(port):
    with socket.create_connection(("127.0.0.1", port), timeout=0.2):
        return True


def main():
    engine_port, http_port = unused_port(), unused_port()
    while engine_port == http_port:
        http_port = unused_port()
    queue = f"smoke-{uuid.uuid4()}"
    processes, logs = [], []
    with tempfile.TemporaryDirectory(prefix="trading-smoke-") as directory:
        database = Path(directory) / "db.sqlite"
        common = dict(os.environ, APP_AMQP__CHANNEL=queue,
                      APP_AMQP__CONSUMER_TAG=queue, APP_DATABASE__FILE=str(database),
                      APP_AMQP__HOST="127.0.0.1", APP_AMQP__PORT="5672",
                      APP_AMQP__USERNAME="admin", APP_AMQP__PASSWORD="pass")
        try:
            for service, overrides in [
                ("engine", {"APP_APPLICATION__PORT": str(engine_port)}),
                ("message-persistor", {}),
                ("api-gateway", {"APP_APPLICATION__PORT": str(http_port),
                                 "APP_ENGINE__PORT": str(engine_port)}),
            ]:
                log = open(Path(directory) / f"{service}.log", "w+")
                logs.append(log)
                processes.append(subprocess.Popen([str(ROOT / "target/debug" / service)],
                    cwd=ROOT / service, env=dict(common, **overrides), stdout=log, stderr=log))
            wait_for(lambda: port_ready(engine_port) and port_ready(http_port), processes)
            # Oversized, malformed, and missing-payload frames must not kill the engine.
            for frame in [struct.pack("!I", 0xFFFFFFFF), b"\0\0\0\1\xff", b"\0\0\0\2\x08\x01"]:
                with socket.create_connection(("127.0.0.1", engine_port), timeout=2) as sock:
                    sock.sendall(frame)
                    try:
                        assert sock.recv(1) == b"", "bad connection remained open"
                    except ConnectionResetError:
                        pass

            def request(method, body):
                req = urllib.request.Request(f"http://127.0.0.1:{http_port}/orders",
                    data=json.dumps(body).encode(), method=method,
                    headers={"Content-Type": "application/json"})
                try:
                    with urllib.request.urlopen(req, timeout=5) as response:
                        return response.status
                except urllib.error.HTTPError as error:
                    return error.code

            def order(side, price, quantity):
                return dict(user_id=side, side=side, price=price, quantity=quantity,
                            base_currency="BTC", quote_currency="USD")

            assert request("POST", order(0, 100, 1)) == 400
            assert request("POST", order(2, 5_000_000_000, 4_000_000_000)) == 202
            assert request("POST", order(1, 5_000_000_000, 4_000_000_000)) == 202
            assert request("POST", order(1, 100, 5)) == 202
            assert request("DELETE", {"order_id": 3}) == 202

            def persisted():
                with sqlite3.connect(database) as connection:
                    trades = connection.execute("SELECT maker_order_id, taker_order_id, price, filled_qty FROM trades").fetchall()
                    orders = connection.execute("SELECT order_id, cancelled FROM orders ORDER BY order_id").fetchall()
                    receipts = connection.execute("SELECT count(*) FROM processed_events").fetchone()[0]
                return trades == [(1, 2, 5_000_000_000, 4_000_000_000)] and orders == [(1, 0), (2, 0), (3, 1)] and receipts == 5

            wait_for(persisted, processes)
            # Restart just the persistor: migrations are repeatable and committed data survives.
            processes[1].terminate()
            processes[1].wait(timeout=5)
            # Replay committed events with their original AMQP message IDs, as after a lost ack.
            with sqlite3.connect(database) as connection:
                replay = connection.execute("SELECT event_id, payload FROM processed_events").fetchall()
            for event_id, payload in replay:
                publish = urllib.request.Request("http://127.0.0.1:15672/api/exchanges/%2F/amq.default/publish",
                    data=json.dumps({"properties": {"message_id": event_id, "delivery_mode": 2},
                        "routing_key": queue, "payload": base64.b64encode(payload).decode(),
                        "payload_encoding": "base64"}).encode(), method="POST",
                    headers={"Authorization": "Basic " + base64.b64encode(b"admin:pass").decode(),
                             "Content-Type": "application/json"})
                with urllib.request.urlopen(publish, timeout=5) as response:
                    assert json.load(response)["routed"]
            processes[1] = subprocess.Popen([str(ROOT / "target/debug/message-persistor")],
                cwd=ROOT / "message-persistor", env=common, stdout=logs[1], stderr=logs[1])
            assert request("POST", order(1, 101, 7)) == 202
            def restarted():
                with sqlite3.connect(database) as connection:
                    return (connection.execute("SELECT count(*) FROM orders").fetchone()[0] == 4
                        and connection.execute("SELECT count(*) FROM trades").fetchone()[0] == 1
                        and connection.execute("SELECT count(*) FROM processed_events").fetchone()[0] == 6)
            wait_for(restarted, processes)
            def market_snapshot():
                with urllib.request.urlopen(f"http://127.0.0.1:{http_port}/market", timeout=2) as response:
                    return json.load(response)
            def market_ready():
                snapshot = market_snapshot()
                return (snapshot["status"] == "live" and snapshot["sequence"] == "6"
                    and snapshot["bids"] == [{"price": "101", "quantity": "7", "order_count": 1}]
                    and snapshot["asks"] == [] and snapshot["open_order_count"] == 1
                    and snapshot["trades"][0]["maker_order_id"] == "1"
                    and snapshot["trades"][0]["price"] == "5000000000")
            wait_for(market_ready, processes)
            # The gateway can reconstruct its book entirely from committed events.
            processes[2].terminate()
            processes[2].wait(timeout=5)
            processes[2] = subprocess.Popen([str(ROOT / "target/debug/api-gateway")],
                cwd=ROOT / "api-gateway", env=dict(common, APP_APPLICATION__PORT=str(http_port),
                    APP_ENGINE__PORT=str(engine_port)), stdout=logs[2], stderr=logs[2])
            wait_for(market_ready, processes)
            with urllib.request.urlopen(f"http://127.0.0.1:{http_port}/market/stream", timeout=5) as stream:
                assert stream.headers["Content-Type"].startswith("text/event-stream")
                def next_book(minimum):
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        line = stream.readline().decode()
                        if not line:
                            raise RuntimeError("market stream closed")
                        if line.startswith("data: "):
                            snapshot = json.loads(line[6:])
                            if int(snapshot["sequence"]) >= minimum:
                                return snapshot
                    raise TimeoutError("market update did not arrive")
                assert next_book(6)["bids"][0]["price"] == "101"
                # Decimal strings preserve the entire supported integer range in browser JSON.
                payload = order(1, "102", "9")
                payload["user_id"] = "9007199254740993"
                assert request("POST", payload) == 202
                snapshot = next_book(7)
                assert snapshot["bids"][0] == {"price": "102", "quantity": "9", "order_count": 1}
                assert snapshot["open_orders"][0]["user_id"] == "9007199254740993"
                assert request("DELETE", {"order_id": "5"}) == 202
                assert next_book(8)["bids"] == [{"price": "101", "quantity": "7", "order_count": 1}]
            def tracked_order(side, price, quantity):
                req = urllib.request.Request(f"http://127.0.0.1:{http_port}/orders",
                    data=json.dumps(order(side, price, quantity)).encode(), method="POST",
                    headers={"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=5) as response:
                    assert response.status == 202
                    return json.load(response)["client_order_id"]
            def progress(client_id, details=True):
                with urllib.request.urlopen(f"http://127.0.0.1:{http_port}/orders/status/{client_id}?details={str(details).lower()}", timeout=5) as response:
                    return json.load(response)
            def reached(client_id, status):
                result = progress(client_id)
                return result if result["status"] == status else None
            tracked = tracked_order(1, 103, 10)
            wait_for(lambda: reached(tracked, "open"), processes)
            assert request("POST", order(2, 103, 4)) == 202
            partial = wait_for(lambda: reached(tracked, "partially_filled"), processes)
            assert (partial["filled"], partial["remaining"], partial["fill_count"]) == ("4", "6", 1)
            assert partial["trades"][0]["quantity"] == "4"
            assert progress(tracked, False)["trades"] == []
            assert request("POST", order(2, 103, 6)) == 202
            filled = wait_for(lambda: reached(tracked, "filled"), processes)
            assert (filled["filled"], filled["remaining"], filled["fill_count"]) == ("10", "0", 2)
            cancelled_id = tracked_order(1, 104, 10)
            assert cancelled_id != tracked
            wait_for(lambda: reached(cancelled_id, "open"), processes)
            assert request("POST", order(2, 104, 3)) == 202
            partial = wait_for(lambda: reached(cancelled_id, "partially_filled"), processes)
            assert request("DELETE", {"order_id": partial["order_id"]}) == 202
            cancelled = wait_for(lambda: reached(cancelled_id, "cancelled"), processes)
            assert (cancelled["filled"], cancelled["remaining"], cancelled["cancelled_quantity"]) == ("3", "0", "7")
            print("PASS: malformed TCP isolation, HTTP validation, limit match, large integers, cancellation, SQLite persistence, duplicate replay, persistor restart, market snapshot, gateway replay, live SSE updates, correlated order progress, partial/full fills, partial cancellation")
        except Exception:
            for log in logs:
                log.flush()
                log.seek(0)
                print(f"--- {Path(log.name).name} ---\n{log.read()}")
            raise
        finally:
            for process in processes:
                if process.poll() is None:
                    process.terminate()
            for process in processes:
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            for log in logs:
                log.close()
            # Delete only this test's unique queue, never a developer's orders queue.
            token = base64.b64encode(b"admin:pass").decode()
            request = urllib.request.Request(f"http://127.0.0.1:15672/api/queues/%2F/{queue}",
                method="DELETE", headers={"Authorization": f"Basic {token}"})
            try:
                urllib.request.urlopen(request, timeout=5).close()
            except (OSError, urllib.error.HTTPError) as error:
                print(f"Queue cleanup failed for {queue}: {error}")


if __name__ == "__main__":
    main()
