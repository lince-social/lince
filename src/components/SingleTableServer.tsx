"use server";

import { prisma } from "@lib/prisma";
import SingleTable from "./SingleTable";

export default async function ServerTableServer(tableName: string) {
  const data = await prisma.$queryRawUnsafe(`SELECT * FROM ${tableName}`);
  return <SingleTable data={data} tableName={tableName} />;
}
