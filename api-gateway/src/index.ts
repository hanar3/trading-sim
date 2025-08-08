import "./amqp";
import { Elysia } from "elysia";
import { orders } from "./orders";

const app = new Elysia()
  .use(orders)
  .get("/", () => "Hello Elysia").listen(3000);

console.log(
  `🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`
);
