"use client";
import { useEffect, useState } from "react";

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
  barType: "Configuration" | "View";
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

function Item({
  itemContent,
  itemStyle,
}: {
  itemContent: string;
  itemStyle: string;
}) {
  const [itemActive, setItemActive] = useState(false);

  if (itemActive) {
    itemStyle = itemStyle + " border-2";
  }

  return (
    <>
      <button className={itemStyle} onClick={() => setItemActive(!itemActive)}>
        {itemContent}
      </button>
    </>
  );
}

function BarItems({ barList, itemStyle }: BarItems) {
  return <div className="flex flex-1"></div>;
}

export default function Bar({ barList, barType }: Bar) {
  let key: number = 0;

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
          {barList.map((item) => (
            <Item itemStyle={itemStyle} itemContent={item} key={key++} />
          ))}
          <AddBarItemButton barType={barType} onAdd={() => alert(barType)} />
        </div>
      </div>
    </>
  );
}
