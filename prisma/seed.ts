import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

async function main() {
  const countConfiguration = await prisma.configuration.count();
  if (countConfiguration === 0) {
    const configurations = await prisma.configuration.createMany({
      data: [
        {
          quantity: 0,
          configurationName: "Initial Configuration",
          columnInformation: "Short",
          views: { "Initial View": true, "Second View": false },
        },
        {
          quantity: 0,
          configurationName: "Third Configuration",
          columnInformation: "Short",
          views: {
            "Initial View": true,
            "Second View": true,
            "Third View": false,
          },
        },

        {
          quantity: 1,
          configurationName: "Second Configuration",
          columnInformation: "Verbose",
          views: { "Third View": true, "Second View": false },
        },
      ],
    });
    console.log({ configurations });
  }

  const countViews = await prisma.view.count();
  if (countViews === 0) {
    const views = await prisma.view.createMany({
      data: [
        { viewName: "Initial View", viewQuery: "SELECT * FROM record" },
        { viewName: "Second View", viewQuery: "SELECT head FROM record" },
        {
          viewName: "Third View",
          viewQuery: "SELECT id, quantity FROM record",
        },
      ],
    });
    console.log({ views });
  }

  const countRecord = await prisma.record.count();
  if (countRecord === 0) {
    const records = await prisma.record.createMany({
      data: [{ head: "Apple" }, { head: "Lemon" }],
    });
    console.log({ records });
  }
}
main()
  .then(() => prisma.$disconnect())
  .catch(async (e) => {
    console.error(e);
    await prisma.$disconnect();
    process.exit(1);
  });
