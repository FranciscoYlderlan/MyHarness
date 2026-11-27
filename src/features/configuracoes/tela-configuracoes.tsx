// TEL-06 Configurações: abre sobre a TEL-01 pelo botão Configurações (NAV-15) e fechar
// volta a ela (NAV-16). Por enquanto só as seções, cada uma com o espaço reservado.
import { Dialog } from "radix-ui";
import { SecaoCofres } from "@/features/cofres";
import { SecaoLimites } from "./secao-limites";
import { SecaoModelos } from "./secao-modelos";
import { SecaoPrompts } from "./secao-prompts";

export function TelaConfiguracoes({
  aberta,
  aoMudar,
}: {
  aberta: boolean;
  aoMudar: (aberta: boolean) => void;
}) {
  return (
    <Dialog.Root open={aberta} onOpenChange={aoMudar}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 max-h-[calc(100vh-2rem)] w-[min(48rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-xl border bg-background p-6 shadow-lg">
          <Dialog.Title className="text-lg font-semibold">Configurações</Dialog.Title>
          <Dialog.Description className="text-sm text-muted-foreground">
            Ajuste os prompts de papel, os modelos, os limites e os cofres do projeto.
          </Dialog.Description>
          <div className="mt-4 space-y-6">
            <SecaoPrompts />
            <SecaoModelos />
            <SecaoLimites />
            <SecaoCofres />
          </div>
          <Dialog.Close className="absolute top-4 right-4 text-sm text-muted-foreground hover:text-foreground">
            Fechar
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
