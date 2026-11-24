// TEL-02 Integrações: abre sobre a TEL-01, já checando o status das três (estado inicial
// "integracoes"), com um CMP-11 por integração. Fechar volta à TEL-01 (NAV-04).
import { Dialog } from "radix-ui";
import { useEffect } from "react";
import { CartaoIntegracao } from "./cartao-integracao";
import { useIntegracoes } from "./contexto";
import { ORDEM_DAS_INTEGRACOES } from "./mensagens";

export function PainelIntegracoes({
  aberto,
  aoMudar,
}: {
  aberto: boolean;
  aoMudar: (aberto: boolean) => void;
}) {
  const { checarTodas } = useIntegracoes();

  useEffect(() => {
    if (aberto) checarTodas();
  }, [aberto, checarTodas]);

  return (
    <Dialog.Root open={aberto} onOpenChange={aoMudar}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 w-[min(32rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-xl border bg-background p-6 shadow-lg">
          <Dialog.Title className="text-lg font-semibold">Integrações</Dialog.Title>
          <Dialog.Description className="text-sm text-muted-foreground">
            Conecte e confira o Claude CLI, a Obsidian CLI e o GitHub.
          </Dialog.Description>
          <div className="mt-4 space-y-3">
            {ORDEM_DAS_INTEGRACOES.map((tipo) => (
              <CartaoIntegracao key={tipo} tipo={tipo} />
            ))}
          </div>
          <Dialog.Close className="absolute top-4 right-4 text-sm text-muted-foreground hover:text-foreground">
            Fechar
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
