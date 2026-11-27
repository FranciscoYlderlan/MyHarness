// Link externo da TEL-02 (device do GitHub, instalação do Claude CLI, `obsidian://open`):
// na janela do app, o clique abre o destino pelo sistema, nunca dentro da janela.
import type { ReactNode } from "react";
import { abrirLinkExterno } from "@/lib/abrir-link-externo";

export function LinkExterno({ href, children }: { href: string; children: ReactNode }) {
  const web = /^https?:\/\//.test(href);
  return (
    <a
      href={href}
      {...(web ? { target: "_blank", rel: "noreferrer" } : {})}
      className="text-primary underline"
      onClick={(evento) => {
        if (abrirLinkExterno(href)) evento.preventDefault();
      }}
    >
      {children}
    </a>
  );
}
