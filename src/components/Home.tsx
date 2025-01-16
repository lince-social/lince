import Configurations from "@/components/Configurations";
import Main from "@/components/Main";
import { prisma } from "@lib/prisma";

export default async function Home() {
  const inactiveConfigs = await prisma.configuration.findMany({
    where: { quantity: 0 },
    orderBy: { quantity: "desc" },
  });

  const activeConfig = await prisma.configuration.findFirst({
    where: { quantity: 1 },
  });

  const activeViews = Object.keys(activeConfig?.views || {}).filter(
    (key) => activeConfig.views[key] === true,
  );

  const queries = await prisma.view.findMany({
    select: { viewQuery: true },
    where: { viewName: { in: activeViews } },
  });

  let tableNames = [];
  let splitquery;
  queries.map((view) => {
    splitquery = view.viewQuery.split(" ");
    splitquery.map((block, index) => {
      if (block.toUpperCase() === "FROM") {
        tableNames.push(splitquery[index + 1]);
      }
    });
  });

  const data = await Promise.all(
    queries.map((query) => prisma.$queryRawUnsafe(query.viewQuery)),
  );

  return (
    <>
      <div className="space-y-3">
        <Configurations
          activeConfig={activeConfig}
          inactiveConfigs={inactiveConfigs}
        />
        <Main data={data} tableNames={tableNames} />
      </div>
    </>
  );
}
