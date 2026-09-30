Claude CLI falso para testes (binário `fake-claude`, membro do workspace). Nunca chama a rede nem a CLI real.
Escreve três linhas JSON no estilo `--output-format stream-json`: `system`/`init` (modelo e modo de permissão recebidos), `fake_args` (os argumentos recebidos, na ordem, só os nomes das variáveis de ambiente e, em `path`, as pastas do `PATH` recebido, o único valor que sai) e `result`/`success`; sai com 0.
`tests/chamada_montada.rs` roda o binário com a chamada de `montar_chamada` (núcleo) e confere argumentos e ambiente sem token do GitHub (inclusive o `PATH` sem a pasta com token).
