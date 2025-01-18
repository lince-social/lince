"use server";
import { prisma } from "@lib/prisma";
import { revalidatePath } from "next/cache";

export async function handleDeleteData(id, tableName) {
  try {
    await prisma[tableName].delete({
      where: {
        id: Number(id),
      },
    });
    revalidatePath("/");
  } catch (error) {
    console.error("Error in API:", error);
  }
}
