import * as elements from "typed-html";
import {
  CreateDataComponent,
  DeleteDataComponent,
  PrintHelpComponent,
  QueryInputComponent,
  ReadDataComponent,
  RunSqlFileComponent,
  UpdateDataComponent,
  ZeroRecordQuantity,
} from "./CrudOperation";
import { saveDatabase } from "../../../db/startup";
import { ConfigurationChange } from "../configurations/CrudConfigurations";
import Body, { FatherBody } from "../sections/Body";
import { isNumericString } from "elysia/utils";

export async function HandleOperation(operation: string) {


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
    case operation === "karma_consequence":
      return await ReadDataComponent("karma_consequence");
    case operation === "karma_condition":
      return await ReadDataComponent("karma_condition");
  }

  let table;
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
    case /10/.test(operation):
      table = "karma_consequence";
      break;
    case /11/.test(operation):
      table = "karma_condition";
      break;
    default:
      table = "record";
  }

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

export default async function OperationComponent(body: any) {
  const { operation } = await body;

  if (isNumericString(operation)) {
    await ZeroRecordQuantity(operation)
    return Body()
  }

  const HandledOperation = await HandleOperation(operation);
  return (
    <FatherBody>
      <div class="fixed inset-0 z-30 flex items-center justify-center">
        <div
          class="flex align-center rounded justify-center focus:outline-none focus:ring-0"
          hx-get="/"
          hx-trigger="keydown[key === 'Escape'] from:body"
          hx-target="#body"
        >
          (Press Escape to remove)
          {HandledOperation}
        </div>
      </div>
    </FatherBody>
  );
}
