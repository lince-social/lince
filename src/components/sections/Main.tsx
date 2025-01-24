return (
  <div className="flex m-4 space-x-4">
    {data.map((tableData, index: number) => (
      <div key={index} className="space-y-2">
        <Table data={tableData} tableName={tableNames[index]} />
      </div>
    ))}
  </div>
);
}

