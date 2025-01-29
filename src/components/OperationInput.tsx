import * as elements from "typed-html";

export default function OperationInput() {
  return (
    <div>
      <input
        id="OperationInput"
        name="operation"
        placeholder="Operation here..."
        hx-post="/operation"
        hx-target="#body"
        class="rounded text-black mb-2"
      />
    </div>
  );
}
