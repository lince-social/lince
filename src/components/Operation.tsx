import * as elements from "typed-html";
import { saveDatabase } from "../../db/startup";
import {
  CreateDataComponent,
  DeleteDataComponent,
  PrintHelpComponent,
  ReadDataComponent,
  QueryInputComponent,
  RunSqlFileComponent,
  UpdateDataComponent,
  ConfigurationChange,
} from "./Crud";
import { FatherBody } from "../components/sections/Body";

export async function HandleOperation(operation) {
  let table;

  switch (true) {
    case operation === "configuration":
      return await ReadDataComponent("configuration");
    case operation === "configuration_view":
      return await ReadDataComponent("configuration_view");
    case operation === "history":
      return await ReadDataComponent("history");
    case operation === "record":
      return await ReadDataComponent("record");
    case operation === "karma":
      return await ReadDataComponent("karma");
    case operation === "frequency":
      return await ReadDataComponent("frequency");
    case operation === "command":
      return await ReadDataComponent("command");
    case operation === "sum":
      return await ReadDataComponent("sum");
    case operation === "transfer":
      return await ReadDataComponent("transfer");
    case operation === "view":
      return await ReadDataComponent("view");
  }

  // Determine table based on shortcuts
  switch (true) {
    case /0/.test(operation):
      table = "configuration";
      break;
    case /1/.test(operation):
      table = "history";
      break;
    case /2/.test(operation):
      table = "record";
      break;
    case /3/.test(operation):
      table = "karma";
      break;
    case /4/.test(operation):
      table = "frequency";
      break;
    case /5/.test(operation):
      table = "command";
      break;
    case /6/.test(operation):
      table = "sum";
      break;
    case /7/.test(operation):
      table = "transfer";
      break;
    case /8/.test(operation):
      table = "view";
      break;
    case /9/.test(operation):
      table = "configuration_view";
      break;
    default:
      table = "record"; // Default fallback
  }

  // Handle actions
  switch (true) {
    case /c/.test(operation):
      return await CreateDataComponent(table);
    case /r/.test(operation):
      return await ReadDataComponent(table);
    case /u/.test(operation):
      return await UpdateDataComponent(table);
    case /d/.test(operation):
      return await DeleteDataComponent(table);
    case /s/.test(operation):
      saveDatabase();
      return;
    case /a/.test(operation):
      const id = operation.match(/\d+/)?.[0];
      if (id) return ConfigurationChange(id);
      return;
    case /q/.test(operation):
      return await QueryInputComponent();
    case /f/.test(operation):
      return RunSqlFileComponent();
    case /h/.test(operation):
      return PrintHelpComponent();
    default:
      return ReadDataComponent(table);
  }
}

// export async function HandleOperation(operation) {
//   let table;
//   switch (true) {
//     case /0/.test(operation):
//       table = "configuration";
//       break;
//     case /configuration/.test(operation):
//       return await ReadDataComponent("configuration");
//
//     case /1/.test(operation):
//       table = "history";
//       break;
//     case /history/.test(operation):
//       return await ReadDataComponent("history");
//
//     case /2/.test(operation):
//       table = "record";
//       break;
//     case /record/.test(operation):
//       return await ReadDataComponent("record");
//
//     case /3/.test(operation):
//       table = "karma";
//       break;
//     case /karma/.test(operation):
//       return await ReadDataComponent("karma");
//
//     case /4/.test(operation):
//       table = "frequency";
//       break;
//     case /frequency/.test(operation):
//       return await ReadDataComponent("frequency");
//
//     case /5/.test(operation):
//       table = "command";
//       break;
//     case /command/.test(operation):
//       return await ReadDataComponent("command");
//
//     case /6/.test(operation):
//       table = "sum";
//       break;
//     case /sum/.test(operation):
//       return await ReadDataComponent("sum");
//
//     case /7/.test(operation):
//       table = "transfer";
//       break;
//     case /transfer/.test(operation):
//       return await ReadDataComponent("transfer");
//
//     case /8/.test(operation):
//       table = "view";
//       break;
//     case /view/.test(operation):
//       return await ReadDataComponent("view");
//
//     default:
//       table = "record";
//       break;
//   }
//
//   switch (true) {
//     case /c/.test(operation):
//       return await CreateDataComponent(table);
//     case /r/.test(operation):
//       return await ReadDataComponent(table);
//     case /u/.test(operation):
//       return await UpdateDataComponent(table);
//     case /d/.test(operation):
//       return await DeleteDataComponent(table);
//     case /s/.test(operation):
//       saveDatabase();
//       return;
//     case /a/.test(operation):
//       const id = operation.match(/\d+/)?.[0];
//       if (typeof id !== "undefined") return ConfigurationChange(id);
//       return;
//     case /q/.test(operation):
//       return await QueryInputComponent();
//     case /f/.test(operation):
//       return RunSqlFileComponent();
//     case /h/.test(operation):
//       return PrintHelpComponent();
//     default:
//       return ReadDataComponent(table);
//   }
// }

export default async function OperationComponent(body) {
  const { operation } = await body;
  const HandledOperation = await HandleOperation(operation);
  return (
    <FatherBody>
      <div class="flex flex-col justify-center items-center">
        (Press Escape to remove)
        <div
          class="flex align-center rounded justify-center z-30 focus:outline-none focus:ring-0"
          hx-get="/"
          hx-trigger="keydown[key === 'Escape'] from:body"
          hx-target="#body"
        >
          {HandledOperation}
        </div>
      </div>
    </FatherBody>
  );
}
