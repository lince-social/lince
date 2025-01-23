"use server";
import { revalidatePath } from "next/cache";

export default async function handleConfigurationClick(id) {
  try {
    await prisma.configuration.updateMany({
      data: { quantity: 0 },
    });
    await prisma.configuration.update({
      where: { id: Number(id) },
      data: { quantity: 1 },
    });
    revalidatePath("/");
  } catch (error) {
    console.log("Error updating quantities:", error);
  }
}
