"use server";

import { prisma } from "@lib/prisma";
import { revalidatePath } from "next/cache";

export async function handleViewToggle(views, view, index) {
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

export async function handleViewRemove(views, view, configurationId) {
  try {
    const newViews = views.filter((v) => v !== view);
    const viewsObject = Object.fromEntries(
      newViews.map((key) => [key[0], key[1]]),
    );

    await prisma.configuration.updateMany({
      where: { id: configurationId },
      data: { views: viewsObject },
    });
    revalidatePath("/");
  } catch (error) {
    console.log(
      `Error: ${error}, when updating view: ${view}, in configuration with id: ${configurationId}. Views received: ${views}`,
    );
  }
}

export async function handleViewAdd(views, configurationId) {
  try {
    console.log(views, configurationId);

    const queriedViews = await prisma.view.findMany();

    if (queriedViews) {
      console.log(queriedViews);
    }

    revalidatePath("/");
  } catch (error) {
    console.log(
      `Error: ${error}, when creating new view in configuration with id: ${configurationId}. Views received: ${views}`,
    );
  }
}
