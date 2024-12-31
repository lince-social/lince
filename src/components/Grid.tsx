import Table from "./Table";

export default async function Grid() {
  const request = await fetch("http://localhost:3000/api/data");
  const queryList = await request.json();

  let key: number = 0;

  return (
    <>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 p-4">
        {queryList.map((tableData, index) => (
          <Table key={key++} data={tableData} />
        ))}
      </div>
    </>
  );
}
