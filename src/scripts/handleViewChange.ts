"use server";
import { prisma } from "@lib/prisma";
import { revalidatePath } from "next/cache";

export default async function handleViewChange(views, view, index) {
  try {
    const updatedView = [view[0], !view[1]];
    const updatedViews = views.map((v, i) => (i === index ? updatedView : v));
    const viewsObject = updatedViews.reduce((acc, [viewName, state]) => {
      acc[viewName] = state;
      return acc;
    }, {});

    await prisma.configuration.updateMany({
      where: { quantity: 1 },
      data: {
        views: viewsObject,
      },
    });
    revalidatePath("/");
  } catch (error) {
    console.log("Error in updating views: ", error);
  }
}
