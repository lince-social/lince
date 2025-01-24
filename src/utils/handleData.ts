"use server";

import { prisma } from "@lib/prisma";
import { revalidatePath } from "next/cache";

export async function handleDataCreate(tableName: string, row: any[]) {
  return console.log(tableName, row);
}

export async function handleDataDelete(idArray: number[], tableName: string) {
  try {
    await prisma[tableName].deleteMany({
      where: {
        id: {
          in: idArray,
        },
      },
    });
    revalidatePath("/");
    console.log(typeof idArray);
    console.log(idArray);
  } catch (error) {
    console.error("Error in API:", error);
  }
}

export async function handleDataRead(tableName) {
  const data = prisma.$queryRawUnsafe(`SELECT * FROM ${tableName}`);
  return data;
}

export async function handleDataUpdate(tableName, set, where) {
  return console.log(tableName, set, where);
}
