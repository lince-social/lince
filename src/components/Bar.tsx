"use client";
import { useState } from "react";

interface BarItems {
  itemStyle: string;
  barList: string[];
}

interface Bar {
  barList: string[];
  barType: "configuration" | "view";
}

interface BarItem {
  name: string;
  active: boolean;
}

interface BarProps {
  barList: BarItem[];
  barType;
}

function AddBarItemButton({ onAdd }: { onAdd: () => void }) {
  return (
    <button
      onClick={onAdd}
      className="bg-gray-600 hover:bg-gray-500 h-6 w-6 ml-1 rounded"
    >
      +
    </button>
  );
}

function BarItems({ barList, itemStyle }: BarItems) {
  const [itemActive, setItemActive] = useState(false);
  let key: number = 0;
  return (
    <div className="flex flex-1">
      {barList.map((item) => (
        <p className={itemStyle} key={key++}>
          {item}
        </p>
      ))}
    </div>
  );
}

export default function Bar({ barList, barType }: Bar) {
  let barStyle: string = "flex flex-1 max-w-min p-1 rounded items-center ";
  barStyle += barType === "configuration" ? "bg-red-700" : "bg-blue-700";

  let itemStyle: string = "space-x-1 max-w-min m-1 p-1 rounded ";
  itemStyle +=
    barType === "configuration"
      ? "bg-red-500 hover:bg-red-400"
      : "bg-blue-500 hover:bg-blue-400";

  return (
    <>
      <div className="flex flex-1 items-center">
        <div className={barStyle}>
          <BarItems barList={barList} itemStyle={itemStyle} />
          <AddBarItemButton barType={barType} onAdd={() => alert(barType)} />
        </div>
      </div>
    </>
  );
}
