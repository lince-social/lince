function Table({ view }: { view: string }) {
  return (
    <>
      <table className="m-1 bg-green-500 rounded hover:rounded-none">
        <tbody>
          <tr>
            <td className="p-1">{view}</td>
          </tr>
        </tbody>
      </table>
    </>
  );
}

function Tables({ configuration_id }: { configuration_id: number }) {
  let key: number = 0;

  const viewsList: string[] =
    configuration_id === 0 ? ["select", "eoihoih"] : [];

  return (
    <div className="m-2 flex flex-1 flex-wrap bg-green-900">
      {viewsList.map((view) => (
        <Table view={view} key={key++} />
      ))}
    </div>
  );
}

export default function Main() {
  const configuration_id: number = 0;
  return (
    <>
      <section className="flex">
        <Tables configuration_id={configuration_id}></Tables>
      </section>
    </>
  );
}
