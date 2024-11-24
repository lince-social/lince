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
  const { id } = req.query; // Query parameter for the view ID

  try {
    const viewResult = await pool.query('SELECT view FROM views WHERE id = $1', [id]);

    if (viewResult.rowCount === 0) {
      return res.status(404).json({ message: 'View not found' });
    }

    const query = viewResult.rows[0].view;
    const { rows } = await pool.query(query); // Execute the query stored in the view
    res.status(200).json(rows);
  } catch (error) {
    console.error('Error executing query:', error);
    res.status(500).json({ message: 'Error executing query' });
  }
}
