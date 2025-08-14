import long from "long";
import { Elysia, t } from "elysia";
import { amqpChannel } from "./amqp";
import { WireMessage, Side, sideFromJSON } from "./generated/proto/trading";

export const orders = new Elysia({
	prefix: "orders",
	name: "Orders"
}).post('/', ({ body }) => {
	const order = WireMessage.create<WireMessage>({
		placeLimitOrder: {
			userId: long.fromInt(1),
			side: sideFromJSON(body.side),
			price: long.fromInt(body.price),
			quantity: long.fromInt(body.quantity)
		}
	});
	const encoded = WireMessage.encode(order).finish();

	const ok = amqpChannel.sendToQueue('orders', Buffer.from(encoded))
	return { ok }
}, {
	body: t.Object({
		side: t.Enum({
			Buy: 1,
			Sell: 2
		}),
		quantity: t.Number(),
		price: t.Number(),
	})
})
