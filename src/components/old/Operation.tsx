export default async function OperationFormModal(params) {
  const awaited = await params;
  const operationInput = awaited.operationInput;
  function handleOperation(event) {
    event.preventDefault();
    const form = event.currentTarget;
    const formData = new FormData(form);
    form.reset();
    const data: string = formData.get("operationInput");

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
      case /c/.test(data):
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
        router.push(`/table/${table}`);
        break;
      default:
        console.log("No operation");
        break;
    }
  }
  return (
    <>
      <div>
        <p>Operation Page Modal</p>
        <p>{operationInput}</p>
      </div>
    </>
  );
}
