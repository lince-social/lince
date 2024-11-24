import { NextApiRequest, NextApiResponse } from 'next';
import { Pool } from 'pg';

const pool = new Pool({
  user: 'postgres',
  host: 'localhost',
  database: 'lince',
  password: '1',
  port: 5432,
});

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    const { rows } = await pool.query('SELECT * FROM record');
    res.status(200).json(rows);
  } catch (error) {
    console.error('Database query error:', error);
    res.status(500).json({ message: 'Error fetching data' });
  }
}

