import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getSnapshot, type Snapshot } from "./market";
const key = ["market"] as const;

export function useMarket() {
  const client = useQueryClient();
  const [connection, setConnection] = useState<
    "connecting" | "connected" | "reconnecting"
  >("connecting");
  const query = useQuery({
    queryKey: key,
    queryFn: ({ signal }) => getSnapshot(signal),
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    retry: 2,
  });
  const ready = query.isSuccess;
  useEffect(() => {
    if (!ready) return;
    const events = new EventSource("/api/market/stream");
    events.onopen = () => setConnection("connected");
    events.onerror = () => setConnection("reconnecting");
    events.addEventListener("book", (event: MessageEvent<string>) => {
      try {
        const snapshot = JSON.parse(event.data) as Snapshot;
        client.setQueryData<Snapshot>(key, snapshot);
      } catch {
        setConnection("reconnecting");
        events.close();
      }
    });
    return () => events.close();
  }, [client, ready]);
  return {
    ...query,
    connection,
    live: connection === "connected" && query.data?.status === "live",
  };
}
