import Table from "./Table";

export default async function Grid() {
  const request = await fetch("http://localhost:3000/api/data");
  const queryList = await request.json();

  return (
    <>
      <div className="flex space-x-1">
        {queryList.map((tableData, index) => (
          <Table key={index} data={tableData} />
        ))}
      </div>
    </>
  );
}
