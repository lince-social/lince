import type { NextApiRequest, NextApiResponse } from "next";
import { pool } from '../../../lib/db';

export default async function handler( req: NextApiRequest, res:NextApiResponse) {
  try {
    const client = await pool.connect();
    const result = await client.query('SELECT * FROM record');
    client.release();
    res.status(200).json(result.rows);
  }
  catch (err) {
    res.status(500).json({ error: 'Internal Server Error' })
  }
}
