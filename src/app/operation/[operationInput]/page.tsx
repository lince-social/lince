export default async function OperationPageOriginal({ params }) {
  const awaited = await params;
  const op = awaited.operationInput;
  return (
    <>
      <div>
        <p>Original Operation Page</p>
        <p>{op}</p>
      </div>
    </>
  );
}
