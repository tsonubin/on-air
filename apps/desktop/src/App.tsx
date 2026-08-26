import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StatusResponse } from "@on-air/api-types";

function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<StatusResponse>("get_status")
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <main>
      <h1>on-air</h1>
      <p>
        {status
          ? `core status: ${status.status} (v${status.version})`
          : "loading core status..."}
      </p>
      {error && <p>Error: {error}</p>}
    </main>
  );
}

export default App;
