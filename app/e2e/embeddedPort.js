import net from "node:net";

// The embedded WebDriver of the app listens on this one fixed port. A second run that starts while another app holds it
// talks to that app, so every wdio config refuses to start instead.
export const embeddedPort = 4445;

const portTaken = () =>
  new Promise((resolve) => {
    const socket = net.connect(embeddedPort, "127.0.0.1");
    socket.once("connect", () => (socket.destroy(), resolve(true)));
    socket.once("error", () => resolve(false));
  });

export async function refuseWhenPortTaken() {
  if (!(await portTaken())) return;
  console.error(
    `Port ${embeddedPort} is already taken by another app with the embedded WebDriver. This run would drive that app instead of its own and report its state as the result. ` +
      `Find the holder with \`lsof -nP -iTCP:${embeddedPort} -sTCP:LISTEN\` (Windows: \`netstat -ano | findstr :${embeddedPort}\`), quit that app, then run again.`,
  );
  process.exit(1);
}
