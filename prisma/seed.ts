import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

async function main() {
  const countConfiguration = await prisma.configuration.count();
  if (countConfiguration === 0) {
    const configuration = await prisma.configuration.create({
      data: {
        configurationName: "Initial Configuration",
        columnInformation: "Short",
      },
    });
    console.log({ configuration });
  }

  const countViews = await prisma.views.count();
  if (countViews === 0) {
    const view = await prisma.views.createMany({
      data: [
        {
          viewName: "Initial View",
        },
        { viewName: "Second View" },
      ],
    });
    console.log({ view });
  }

  const countRecord = await prisma.record.count();
  if (countRecord === 0) {
    const view = await prisma.record.create({
      data: { head: "Maçã" },
    });
    console.log({ view });
  }
}
main()
  .then(() => prisma.$disconnect())
  .catch(async (e) => {
    console.error(e);
    await prisma.$disconnect();
    process.exit(1);
  });
