import { MainWindow } from "./main-window";
import { Providers } from "./providers";

export function App() {
  return (
    <Providers>
      <MainWindow />
    </Providers>
  );
}
