import "./amqp";
import { Elysia } from "elysia";
import { orders } from "./orders";
import swagger from "@elysiajs/swagger";
const app = new Elysia()
  .use(swagger())
  .use(orders)
  .get("/", () => "Hello Elysia").listen(3000);

console.log(
  `🦊 Elysia is running at ${app.server?.hostname}:${app.server?.port}`
);
