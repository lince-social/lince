"use client";

import React from "react";
import Table from "@/components/table/Table";

interface MainProps {
  data: Array<Record<string, any>[]>;
}

export default function Main({
  data,
  tableNames,
}: {
  data: React.FC<MainProps>;
  tableNames: string[];
}) {
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
