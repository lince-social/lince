"use server";
import SingleTable from "@/components/SingleTable";
import { prisma } from "@lib/prisma";

export default async function Page({ params }) {
  const awaitedParams = await params;
  const tableName = await awaitedParams.tableName;
  // return <pre>{JSON.stringify(tableName)}</pre>;

  // const tableName = awaitedParams.tableName;
  const data = await prisma.$queryRawUnsafe(`SELECT * FROM ${tableName}`);
  return <SingleTable data={data} tableName={tableName} />;
}
