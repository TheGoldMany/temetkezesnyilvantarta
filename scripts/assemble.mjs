// Builds the Vercel output folder: the website at the root, the app under /app/.
import { cpSync, mkdirSync, rmSync } from "node:fs";

rmSync("public", { recursive: true, force: true });
mkdirSync("public", { recursive: true });
cpSync("website", "public", { recursive: true });
cpSync("dist", "public/app", { recursive: true });
console.log("public/ assembled: website + app");
