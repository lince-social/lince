// import Home from "@/components/section/Home";

import { sql } from "bun";

export default async function Page() {
  const data = await sql`SELECT * FROM configuration`;
  return (
    <>
      <pre>{JSON.stringify(data)}</pre>
    </>
  );
}
