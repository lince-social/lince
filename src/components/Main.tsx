"use client";

import { useState } from "react";

export default function Main({ data }) {
  return (
    <>
      <div>{JSON.stringify(data)}</div>
    </>
  );
}
