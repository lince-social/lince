'use client'
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
        setData(result);
        setLoading(false);
      } catch (err) {
        console.error('Error fetching data:', err);
      }
    }

    fetchData(); // Initial fetch
    interval = setInterval(fetchData, 10000);

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

// import { Client } from "pg";
// import fs from "fs";
// import path from "path";

// interface DBConfig {
//   host?: string;
//   user?: string;
//   database?: string;
//   password?: string;
//   port?: number;
// }

// const defaultConfig: DBConfig = {
//   host: "localhost",
//   user: "postgres",
//   database: "lince",
//   password: "1",
//   port: 5432,
// };

// // Create connection
// export const createConnectionObject = (config: DBConfig = defaultConfig) => {
//   return new Client(config);
// };

// // Execute SQL command
// export const executeSqlCommand = async (command: string, database: string = "lince") => {
//   const client = createConnectionObject({ ...defaultConfig, database });
//   await client.connect();

//   try {
//     const res = await client.query(command);
//     if (command.trim().toUpperCase().startsWith("SELECT")) {
//       return res.rows; // Return rows if it's a SELECT query
//     }
//     return true;
//   } finally {
//     await client.end();
//   }
// };

// // Check if database exists
// export const checkExistsDB = async (): Promise<boolean> => {
//   const client = createConnectionObject();
//   await client.connect();

//   try {
//     const res = await client.query("SELECT datname FROM pg_database WHERE datname = 'lince'");
//     return res.rowCount > 0;
//   } finally {
//     await client.end();
//   }
// };

// // Dump database
// export const dumpDB = async () => {
//   const defaultPath = path.resolve(__dirname, "../../db/lince.sql");
//   const configPath = path.resolve(process.env.HOME || "", ".config/lince/lince.sql");
//   const outputPath = fs.existsSync(configPath) ? configPath : defaultPath;

//   const command = `pg_dump --data-only --no-comments --no-owner --no-privileges -U postgres --no-password -F plain -f ${outputPath} lince -h localhost -p 5432`;

//   return execShellCommand(command);
// };

// // Drop database
// export const dropDB = async () => {
//   return executeSqlCommand("DROP DATABASE lince", undefined);
// };

// // Create database
// export const createDB = async () => {
//   const client = createConnectionObject();
//   await client.connect();

//   try {
//     await client.query("CREATE DATABASE lince");
//     return true;
//   } finally {
//     await client.end();
//   }
// };

// // Apply schema to database
// export const schemeDB = async () => {
//   const schemaPath = path.resolve(__dirname, "../../db/schema.sql");
//   const schemaSQL = fs.readFileSync(schemaPath, "utf-8");

//   return executeSqlCommand(schemaSQL);
// };

// // Restore database
// export const restoreDB = async () => {
//   const defaultPath = path.resolve(__dirname, "../../db/lince.sql");
//   const configPath = path.resolve(process.env.HOME || "", ".config/lince/lince.sql");
//   const inputPath = fs.existsSync(configPath) ? configPath : defaultPath;

//   const command = `psql -h localhost -d lince -U postgres < ${inputPath}`;

//   return execShellCommand(command);
// };

// // Insert if not exists
// export const insertIfNotDB = async () => {
//   const insertPath = path.resolve(__dirname, "../../db/insert_ifnot.sql");
//   const insertSQL = fs.readFileSync(insertPath, "utf-8");

//   return executeSqlCommand(insertSQL);
// };
