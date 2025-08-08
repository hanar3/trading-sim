import amqplib from "amqplib";

export const amqpConn = await amqplib.connect({
	username: "admin",
	password: "pass",
});
export const amqpChannel = await amqpConn.createChannel();

await amqpChannel.assertQueue("orders");

console.log("AMQP Ready");
