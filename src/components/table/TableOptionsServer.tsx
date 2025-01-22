import { prisma } from "@lib/prisma";
import TableOptions from "./TableOptions";

export default async function TablesOptionsServer() {
  const tableNames =
    await prisma.$queryRaw`SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'`;
  return <TableOptions tableNames={tableNames} />;
}
