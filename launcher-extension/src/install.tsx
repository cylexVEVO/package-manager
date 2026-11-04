import { Action, ActionPanel, Alert, confirmAlert, showToast, Toast, closeMainWindow, List, PopToRootType } from "@vicinae/api";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { useEffect, useRef, useState } from "react";

type App = { id: number; name: string; icon: string | null; installed: boolean | null };
type Status = "Installing" | "Installed ✓" | "Uninstalling" | "Uninstalled" | "Failed";
const bridgePath = join(__dirname, "bin", "pm-extension-bridge");
const noIcon = join(__dirname, "assets", "no-icon.webp");

export default function InstallApps() {
  const [query, setQuery] = useState("");
  const [apps, setApps] = useState<App[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [statuses, setStatuses] = useState<Record<number, Status>>({});
  const search = useRef<ChildProcessWithoutNullStreams | null>(null);
  const latestQuery = useRef(query);
  const installing = useRef(new Set<number>());
  latestQuery.current = query;

  useEffect(() => {
    let active = true;
    const child = spawn(bridgePath, ["search"]);
    search.current = child;
    const fail = () => {
      if (!active) return;
      setApps([]);
      setFailed(true);
      setLoading(false);
    };
    const lines = createInterface({ input: child.stdout });
    lines.on("line", (line) => {
      try {
        const reply = JSON.parse(line) as { query: string; apps?: App[]; error?: boolean };
        if (!active || reply.query !== latestQuery.current) return;
        setApps(reply.apps ?? []);
        setFailed(Boolean(reply.error));
        setLoading(false);
      } catch {
        fail();
      }
    });
    child.stderr.on("data", (data) => console.error(String(data)));
    child.on("error", fail);
    child.on("exit", fail);
    child.stdin.on("error", fail);
    return () => {
      active = false;
      search.current = null;
      lines.close();
      child.kill();
    };
  }, []);

  useEffect(() => {
    const child = search.current;
    setApps([]);
    if (!child || child.exitCode !== null || child.stdin.destroyed) {
      setFailed(true);
      setLoading(false);
      return;
    }
    setFailed(false);
    setLoading(true);
    child.stdin.write(`${JSON.stringify(query)}\n`);
  }, [query]);

  async function manage(app: App, operation: "install" | "uninstall") {
    if (installing.current.has(app.id)) return;
    installing.current.add(app.id);
    if (operation === "uninstall" && !(await confirmAlert({
      title: `Uninstall ${app.name}?`,
      message: "Remove this application? Personal app data will be kept where supported by the package manager.",
      primaryAction: { title: "Uninstall", style: Alert.ActionStyle.Destructive },
    }))) {
      installing.current.delete(app.id);
      return;
    }
    setStatuses((current) => ({ ...current, [app.id]: operation === "install" ? "Installing" : "Uninstalling" }));
    const finish = (status: Status) => {
      installing.current.delete(app.id);
      setStatuses((current) => ({ ...current, [app.id]: status }));
      if (status !== "Failed") {
        setApps((current) => current.map((item) => item.id === app.id ? { ...item, installed: operation === "install" } : item));
      }
      void showToast({ style: status === "Failed" ? Toast.Style.Failure : Toast.Style.Success,
        title: status === "Failed" ? `Failed to ${operation} ${app.name}` : `${operation === "install" ? "Installed" : "Uninstalled"} ${app.name}` });
    };
    try {
      // Release launcher focus before a package manager opens authentication.
      await closeMainWindow({ popToRootType: PopToRootType.Suspended });
    } catch (error) {
      console.error(error);
      finish("Failed");
      return;
    }
    // Install output is separate from the search protocol, and leaving the view
    // must not terminate the package manager midway through a transaction.
    const child = spawn(bridgePath, [operation, String(app.id)], {
      detached: true,
      stdio: "ignore",
    });
    child.once("error", () => finish("Failed"));
    child.once("exit", (code) => finish(code === 0 ? (operation === "install" ? "Installed ✓" : "Uninstalled") : "Failed"));
    child.unref();
  }

  return (
    <List filtering={false} isLoading={loading} searchBarPlaceholder="Search apps to install or uninstall:" onSearchTextChange={setQuery}>
      <List.EmptyView title={failed ? "Failed" : loading ? "Loading…" : "No apps found"} />
      {apps.map((app) => (
        <List.Item
          key={app.id}
          id={String(app.id)}
          title={app.name}
          icon={app.icon ? { source: app.icon, fallback: noIcon } : noIcon}
          accessories={[{ text: statuses[app.id] ?? (app.installed ? "Installed ✓" : "Install") }]}
          actions={
            <ActionPanel>
              {!installing.current.has(app.id) && (
                (statuses[app.id] === "Installed ✓" || (statuses[app.id] !== "Uninstalled" && app.installed)) ? <Action title="Uninstall" style={Action.Style.Destructive} onAction={() => manage(app, "uninstall")} />
                  : <Action title="Install" onAction={() => manage(app, "install")} />
              )}
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
