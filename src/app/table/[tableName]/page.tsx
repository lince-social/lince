"use server";
import SingleTable from "@/components/SingleTable";
import { prisma } from "@lib/prisma";

export default async function Page({ params }) {
  const awaitedParams = await params;
  const tableName = awaitedParams.tableName;
  const data = await prisma.$queryRawUnsafe(`SELECT * FROM ${tableName}`);

  return (
    <>
      <div className="m-4">
        <SingleTable data={data} tableName={tableName} />
      </div>
    </>
  );
}
