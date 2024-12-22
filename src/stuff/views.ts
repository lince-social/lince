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
  const { viewId } = req.query;

  try {
    if (viewId) {
      // If viewId is provided, execute the corresponding query
      const viewResult = await pool.query('SELECT view FROM views WHERE id = $1', [viewId]);

      if (viewResult.rowCount === 0) {
        return res.status(404).json({ message: 'View not found' });
      }

      const query = viewResult.rows[0].view;
      const { rows } = await pool.query(query); // Execute the query stored in the view
      return res.status(200).json(rows);
    } else {
      // If no viewId is provided, fetch all view names
      const { rows } = await pool.query('SELECT id, view_name FROM views ORDER BY id ASC');
      return res.status(200).json(rows);
    }
  } catch (error) {
    console.error('Error:', error);
    return res.status(500).json({ message: 'Internal server error' });
  }
}

