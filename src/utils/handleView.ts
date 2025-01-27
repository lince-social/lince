import { sql } from "bun";

export async function handleViewToggle(views, view, configurationId) {
  try {
    const jsonviews = JSON.parse(views);
    const jsonview = JSON.parse(view);

    Object.keys(jsonview).forEach((viewName) => {
      jsonviews[viewName] = !jsonview[viewName];
    });

    await sql`UPDATE configuration SET views = ${jsonviews} WHERE id = ${configurationId};`;
  } catch (error) {
    console.log(
      `Error: ${error}, when updating view: ${view}, in configuration with id: ${configurationId}. Views received: ${views}`,
    );
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
  } catch (error) {
    console.log(
      `Error: ${error}, when creating new view in configuration with id: ${configurationId}. Views received: ${views}`,
    );
  }
}
