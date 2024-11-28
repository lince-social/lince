import { Pool } from 'pg';

const pool = new Pool({
  host: 'localhost',
  user: 'postgres',
  // database: 'lince',
  password: '1',
  port: 5432,
});

export const query = async(text: string, params?: any[]) => {
  const client = await pool.connect();
  try {
    const res = await client.query(text, params);
    return res;
  } finally {
    client.release();
  }
};

export const checkDatabaseExists = async (): Promise<boolean> => {
  const res = await query("SELECT datname FROM pg_database WHERE datname = 'lince'");
  return res.rowCount > 0;
};

export const createDatabase = async () => {
  await query('CREATE DATABASE lince');
};

export const deleteDatabase = async () => {
  await query('DELETE DATABASE lince');
};

export async function GET() {
  try {
    const databaseExists = await checkDatabaseExists();
    console.log(databaseExists);
  } catch (error) {
    console.log(error);
  }
}
