# Validação da migração Rust

Data da carga: **20/10/2026 18:38:43 BRT (UTC−3)**, equivalente a 21:38:43 UTC. Trabalho local; não houve deploy, commit ou push.

## Dados reais e testes

Os 7 arquivos de `docs/empenho/` são PDFs SIAFI de 2 páginas, entre 22.423 e 22.936 bytes. Foram processados de verdade por Poppler e pelo parser Rust, sem IA. Os testes conferem UG, número, CNPJ do favorecido separado do emitente, valor total, modalidade e edital. Gestão permanece ausente; UGR não é usada como substituta.

| Empenho | Valor esperado e obtido |
|---|---:|
| 2026NE000290 | R$ 32.450,00 |
| 2026NE000291 | R$ 36.657,30 |
| 2026NE000292 | R$ 13.351,00 |
| 2026NE000293 | R$ 8.514,00 |
| 2026NE000297 | R$ 30.056,91 |
| 2026NE000383 | R$ 15.800,00 |
| 2026NE000534 | R$ 9.875,00 |

`make test`: **19 testes Rust + 2 testes JavaScript aprovados**, sem skips. Inclui PDFs reais/renomeados, arquivo inválido/protegido/sem texto, mais de 100 páginas, arquivo maior que 5 MiB, isolamento de empresa, saldo concorrente, banco legado, autenticação, corpo HTTP grande, origem recusada, fila cheia, cadastro HTTP, cálculo e download. O teste concorrente do pipeline usa 16 tarefas e verifica pico de até 2 workers.

`make lint`: formatação Rust, Clippy de todos os targets com `-D warnings` e sintaxe JS aprovados. `make coverage`: **80,04% das linhas Rust**, incluindo o entrypoint não coberto pelos testes instrumentados; o entrypoint foi exercitado separadamente no navegador, CLI e container. Sem exclusão artificial de código de aplicação. Cobertura de linhas não mede todos os cenários possíveis.

## Interface real no navegador

Servidor release em loopback, com banco temporário e empresa de validação configurada para o CNPJ dos documentos. O SQLite original não foi utilizado pelo teste.

1. Upload simultâneo dos 7 PDFs: 7/7 processados, com valores corretos e aviso de Gestão ausente.
2. Conferência e cadastro de dois empenhos extraídos; cadastro de duas NFs para o empenho selecionado.
3. Tentativa de NF de R$ 40.000,00 contra saldo insuficiente: recusada e mensagem exibida.
4. Download real do ofício: NF de R$ 1.000,00, multa 2%, juros 1% a.m., 19 dias de atraso: multa R$ 20,00, juros R$ 6,33 e total R$ 1.026,33.
5. Seleção do empenho preservada após atualização das tabelas; regressão de paginação reproduzida antes do ajuste e coberta por dois testes JS. A página da versão final abriu sem erros de console; o conflito de saldo esperado retorna HTTP 409.

## Carga com servidor restrito a 2 CPUs

Binário release, afinidade Linux fixada nas CPUs **26 e 27**, 16 clientes de upload, 140 análises dos mesmos 7 PDFs (20 repetições do corpus). São 140 processamentos, não 140 layouts/documentos distintos.

Comando do cliente de carga, com o servidor já iniciado por `taskset`:

```bash
./target/release/examples/carga 8522 140 16 PID_DO_SERVIDOR
```

| Medida | Resultado |
|---|---:|
| Análises concluídas | 140/140 |
| Duração total | 4,291 s |
| Respostas 429 retentadas | 16; nenhum abandono após tentativas |
| Pico de PDFs ativos | 2 |
| Latência de upload p95 | 4,173 s, incluindo espera e retentativas de 2 s |
| Amostras da rota de saúde | 556, todas HTTP 200 |
| Saúde p95 / máxima | 6,426 ms / 114,960 ms |
| Pico de RSS do servidor observado | 12.660 KiB, aproximadamente 12,4 MiB |
| Pico de soma RSS servidor + filhos observado | 45.964 KiB, aproximadamente 44,9 MiB |

Resultado bruto: [validacao/carga-2cpu-2026-10-20.json](validacao/carga-2cpu-2026-10-20.json).

RSS foi amostrado em `/proc`, incluindo os filhos criados por todos os threads. A soma pode contar páginas compartilhadas mais de uma vez; não é PSS nem um teto de memória para qualquer entrada. A medição é específica destes PDFs pequenos. O servidor foi limitado a duas CPUs da máquina local; isso não reproduz frequência, contenção de hypervisor, disco ou rede da VPS. Não se impôs um limite global de RAM nessa carga nativa; os subprocessos mantiveram os limites implementados. O exemplo systemd acrescenta limite global de 2 GiB ao grupo de processos.

SHA-256 do binário nativo utilizado: `86fe69052c0d0c648cfd5ffa29fe272bb6ab572857a281d8755b2306a695d1f8`. Tamanho aproximado: 6,9 MiB. A compilação Docker já estava concluída durante a carga; a validação curta da CLI Docker ocorreu no mesmo intervalo.

## Container e modelos de hospedagem

Imagem `felix-crawler:local` construída com sucesso pelo Dockerfile final. Digest local: `sha256:5e02381e3b45c45d00edfe9093b40220bc5aa71b573f01bac1917f2d34c1e633`.

A CLI da imagem processou os 7 PDFs com os valores esperados, sob `--cpus=2 --memory=512m --pids-limit=64 --read-only --network=none --cap-drop=ALL --security-opt=no-new-privileges`, `/tmp` em tmpfs de 128 MiB e documentos montados somente para leitura. Essa é validação do runtime do container; não um teste de carga HTTP dentro dele.

O modelo systemd passou na verificação sintática usando uma cópia temporária com `ExecStart` apontando para o binário local existente. O caminho final `/opt/felix/felix-server` não foi instalado. O modelo nginx não foi aplicado nem validado contra um nginx em execução. TLS, token real, configuração da empresa e validação na VPS permanecem etapas de hospedagem.

## Revisão e preservação

Revisões independentes Rust, geral e de segurança realizadas. A revisão geral encontrou a troca involuntária de empenho ao paginar, corrigida e testada. Nenhuma falha crítica foi apontada. Observação menor mantida: algumas falhas internas do Poppler por limite de recursos podem retornar a categoria genérica “PDF inválido”, sem diferenciar falta de memória de corrupção; o resultado parcial não é aceito.

Os PDFs originais permanecem sem alterações no git. O banco original conservou a data de modificação de 20/10/2026 17:59:22 BRT e não foi aberto pelos testes de migração. SHA-256 observado ao final: `6ff3ba7a3e83cda182bea8b6e3bdadff0a5e35d12b19986757c1b422c96cc64c`.

Backup dos arquivos Python substituídos: `/tmp/felix-python-before-rust-lirvv8es.tar`; o commit anterior também os conserva. Não há alteração de arquivos no projeto de referência `llama.rs-MI50`. A aplicação atende uma empresa configurada por instância; não inclui login multicliente, OCR, arquivo permanente de PDFs ou consulta à CGU.
