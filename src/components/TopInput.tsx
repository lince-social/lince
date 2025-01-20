"use client";

import { useRouter } from "next/navigation";

export default function TopInput() {
  const router = useRouter();
  function handleOperation(event) {
    event.preventDefault();
    const form = event.currentTarget;
    const formData = new FormData(form);
    form.reset();
    const data = formData.get("operationInput");

    let table;
    switch (true) {
      case /0/.test(data):
        table = "configuration";
        break;
      case /1/.test(data):
        table = "history";
        break;
      case /2/.test(data):
        table = "record";
        break;
      case /3/.test(data):
        table = "karma";
        break;
      case /4/.test(data):
        table = "frequency";
        break;
      case /5/.test(data):
        table = "command";
        break;
      case /6/.test(data):
        table = "sum";
        break;
      case /7/.test(data):
        table = "transfer";
        break;
      case /8/.test(data):
        table = "view";
        break;
      default:
        table = "record";
        break;
    }

    let action;
    switch (true) {
      case /c/gim.test(data):
        action = "create";
        break;
      case /r/.test(data):
        action = "read";
        break;
      case /u/.test(data):
        action = "update";
        break;
      case /d/.test(data):
        action = "delete";
        break;
      case /s/.test(data):
        action = "save";
        break;
      case /a/.test(data):
        action = "activate";
        break;
      case /q/.test(data):
        action = "query";
        break;
      case /f/.test(data):
        action = "sqlFile";
        break;
      case /h/.test(data):
        action = "help";
        break;
      default:
        action = "read";
        break;
    }

    switch (true) {
      case action === "read":
        // console.log(action, table);
        // const data = await handleDataRead(table);
        // console.log(data);
        router.push(`/table/${table}`);
        break;
      default:
        console.log("No operation");
        break;
    }
  }

  return (
    <>
      <div className="">
        <form onSubmit={handleOperation} className="space-x-2">
          <input
            type="text"
            name="operationInput"
            placeholder="Operation"
            className="rounded"
            required
          />
          <button
            className="bg-gray-600 hover:bg-gray-500 rounded pl-1 pr-1"
            type="submit"
          >
            Send
          </button>
        </form>
      </div>
    </>
  );
}
