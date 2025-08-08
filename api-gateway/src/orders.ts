import long from "long";
import { Elysia, t } from "elysia";
import { amqpChannel } from "./amqp";
import { Order } from "./generated/proto/order";

export const orders = new Elysia({
	prefix: "orders",
	name: "Orders"
}).post('/', async ({ body }) => {
	const order = Order.create<Order>({
		quantity: long.fromInt(body.quantity),
		price: long.fromInt(body.price),
		side: body.side,
	});
	const encoded = Order.encode(order).finish();

	const ok = amqpChannel.sendToQueue('orders', Buffer.from(encoded))
	return { ok }
}, {
	body: t.Object({
		side: t.Enum({
			Buy: 0,
			Sell: 1
		}),
		quantity: t.Number(),
		price: t.Number(),
	})
})
