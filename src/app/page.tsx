import { Pool } from 'pg';

const pool = new Pool({
  user: 'postgres',
  host: 'localhost',
  database: 'lince',
  password: '1',
  port: 5432,
});

async function queryDatabase() {
  try {
    const client = await pool.connect();
    const result = await client.query('SELECT * FROM record');
    console.log(result.rows);
    client.release();
  } catch (err) {
    console.error('Error querying database:', err);
  }
}

let data = queryDatabase();

export default function Home() {
  return (
    <div className="">
      <main className="">
        <p>data</p>
      </main>
    </div>
  );
}
