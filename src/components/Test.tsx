"use client";
import { useEffect, useRef, useState } from "react";

export default function Test() {
  const [time, setTime] = useState(new Date());
  const ref = useRef(null);

  useEffect(() => {
    const intervalId = setInterval(() => {
      setTime(new Date());
    }, 1000);
    return () => {
      clearInterval(intervalId);
    };
  }, [time]);

  function handleClick() {
    console.log(ref.current.value);
  }

  return (
    <div className="flex flex-col content-start justify-start">
      <p>{`${time}`}</p>
      <input ref={ref} />
      <button onClick={handleClick}>Click for focus</button>
    </div>
  );
}
