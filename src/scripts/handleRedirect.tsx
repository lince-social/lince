"use client";
import { useRouter } from "next/navigation";

export default function handleRedirect(path: string) {
  const router = useRouter();
  router.push(path);
}
