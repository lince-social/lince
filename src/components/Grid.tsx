import Table from "./Table";

export default async function Grid() {
  const req = await fetch(
    "http://localhost:3000/api/data?viewQuery=SELECT%20*%20FROM%20record",
  );
  const data = await req.json();

  const req2 = await fetch(
    "http://localhost:3000/api/data?viewQuery=SELECT%20head%20FROM%20record",
  );
  const data2 = await req2.json();

  return (
    <>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 p-4">
        <Table data={data} />
        <Table data={data2} />
      </div>
    </>
  );
}
