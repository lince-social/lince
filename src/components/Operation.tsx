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

export async function HandleOperationInput(operationInput) {
  let table;
  switch (true) {
    case /0/.test(operationInput):
      table = "configuration";
      break;
    case /configuration/.test(operationInput):
      return await ReadDataComponent("configuration");

    case /1/.test(operationInput):
      table = "history";
      break;
    case /history/.test(operationInput):
      return await ReadDataComponent("history");

    case /2/.test(operationInput):
      table = "record";
      break;
    case /record/.test(operationInput):
      return await ReadDataComponent("record");

    case /3/.test(operationInput):
      table = "karma";
      break;
    case /karma/.test(operationInput):
      return await ReadDataComponent("karma");

    case /4/.test(operationInput):
      table = "frequency";
      break;
    case /frequency/.test(operationInput):
      return await ReadDataComponent("frequency");

    case /5/.test(operationInput):
      table = "command";
      break;
    case /command/.test(operationInput):
      return await ReadDataComponent("command");

    case /6/.test(operationInput):
      table = "sum";
      break;
    case /sum/.test(operationInput):
      return await ReadDataComponent("sum");

    case /7/.test(operationInput):
      table = "transfer";
      break;
    case /transfer/.test(operationInput):
      return await ReadDataComponent("transfer");

    case /8/.test(operationInput):
      table = "view";
      break;
    case /view/.test(operationInput):
      return await ReadDataComponent("view");

    default:
      table = "record";
      break;
  }

  switch (true) {
    case /c/.test(operationInput):
      return await CreateDataComponent(table);
    case /r/.test(operationInput):
      return await ReadDataComponent(table);
    case /u/.test(operationInput):
      return await UpdateDataComponent(table);
    case /d/.test(operationInput):
      return await DeleteDataComponent(table);
    case /s/.test(operationInput):
      saveDatabase();
      return null;
    case /a/.test(operationInput):
      const id = operationInput.match(/\d+/)?.[0];
      if (typeof id !== "undefined") {
        ConfigurationChange(id);
      }
      return;
    case /q/.test(operationInput):
      return await QueryInputComponent();
    case /f/.test(operationInput):
      return RunSqlFileComponent();
    case /h/.test(operationInput):
      return PrintHelpComponent();
    default:
      return ReadDataComponent(table);
  }
}

export default async function OperationComponent(operationInput) {
  const HandledOperation = await HandleOperationInput(operationInput);
  return (
    <FatherBody>
      <div class="flex flex-col justify-center items-center">
        (Press Escape to remove)
        <div
          class="flex align-center rounded justify-center z-50 focus:outline-none focus:ring-0"
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
