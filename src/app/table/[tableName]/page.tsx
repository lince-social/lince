"use client";

import SingleTableServer from "@/components/SingleTableServer";
import { useParams } from "next/navigation";

export default function Page() {
  const params = useParams();
  return <SingleTableServer tableName={params.tableName} />;
}
