"use client";

import { useRouter } from "next/navigation";

export default function handleOperationClient(action, table) {
  const router = useRouter();
  switch (true) {
    case action === "read":
      router.push(`/table/${table}`);
  }
}
