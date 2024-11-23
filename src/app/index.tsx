import { useEffect, useState } from 'react';

export default function Home() {
  const [data, setData] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let interval: NodeJS.Timer;

    async function fetchData() {
      try {
        const response = await fetch('/api/query');
        const result = await response.json();
        setData(result); // Assuming API returns an array of records
        setLoading(false);
      } catch (err) {
        console.error('Error fetching data:', err);
      }
    }

    fetchData(); // Initial fetch
    interval = setInterval(fetchData, 5000); // Poll every 5 seconds

    return () => clearInterval(interval); // Clean up polling on component unmount
  }, []);

  return (
    <div>
      <main>
        <h1>Records</h1>
        {loading ? (
          <p>Loading...</p>
        ) : (
          <ul>
            {data.map((item, index) => (
              <li key={index}>{JSON.stringify(item)}</li>
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}
