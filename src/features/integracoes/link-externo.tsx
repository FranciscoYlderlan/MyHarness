// Link externo da TEL-02 (device do GitHub, instalação do Claude CLI, `obsidian://open`):
// na janela do app, o clique abre o destino pelo sistema, nunca dentro da janela. Se o
// sistema não abrir, o cartão mostra a falha logo abaixo do link, com o endereço.
import { useState, type ReactNode } from "react";
import { abrirLinkExterno } from "@/lib/abrir-link-externo";

export function LinkExterno({ href, children }: { href: string; children: ReactNode }) {
  const web = /^https?:\/\//.test(href);
  const [falhou, setFalhou] = useState(false);
  return (
    <>
      <a
        href={href}
        {...(web ? { target: "_blank", rel: "noreferrer" } : {})}
        className="text-primary underline"
        onClick={(evento) => {
          setFalhou(false);
          if (abrirLinkExterno(href, () => setFalhou(true))) evento.preventDefault();
        }}
      >
        {children}
      </a>
      {falhou && (
        <span role="alert" className="mt-1 block text-sm text-destructive">
          Não foi possível abrir o link. Abra este endereço manualmente:{" "}
          <code className="font-mono">{href}</code>
        </span>
      )}
    </>
  );
}
