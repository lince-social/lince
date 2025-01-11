import Header from "@/components/Header";
import Main from "@/components/Main";
import { prisma } from "@lib/prisma";

export default async function Home() {
  const activeConfig = await prisma.configuration.findFirst({
    where: { quantity: 1 },
  });
  const inactiveConfigs = await prisma.configuration.findMany({
    where: { quantity: 0 },
    orderBy: { quantity: "desc" },
  });

  // const queries = await prisma.view.findMany({
  //   where: {viewName: /* in activeConfig.views */}
  // })
  //
  // const data = queries.map((query) => {
  // await prisma.$queryRawUnsafe(query)
  // }
  //
  const data = { Hello: true };

  return (
    <>
      <div className="p-2 space-y-3">
        <Header activeConfig={activeConfig} inactiveConfigs={inactiveConfigs} />
        <Main data={data} />
      </div>
    </>
  );
}
