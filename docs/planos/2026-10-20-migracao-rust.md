# Migração Rust e extração direta de empenhos

## Objetivo autorizado

Substituir o runtime Python/Streamlit/Gemini por aplicação Rust que preserve empenhos, notas, saldo, vencimentos e minuta, analisando os PDFs de `docs/empenho` sem IA. Destino: VPS Linux com 2 CPUs e 4 GiB, com uploads simultâneos. Nenhum deploy, commit ou acesso de escrita ao projeto de referência.

## Decisões de implementação

- Workspace Rust 2024 com crates `felix-core` (domínio/parser) e `felix-server` (HTTP, SQLite, processos). Seguir `llama.rs-MI50`: Result/thiserror, unsafe proibido, sem unwrap/expect/panic em produção, regex compiladas uma vez, thin LTO/codegen-units=1. Não copiar otimização `target-cpu=native`: o binário deve ser portátil para a VPS.
- Axum/Tokio para rede; Poppler `pdfinfo`/`pdftotext` como subprocessos sem shell, isolados por `prlimit`. Upload em chunks para temporários RAII. 2 extrações simultâneas; no máximo 8 requisições de análise admitidas; excesso retorna 429 e Retry-After. Limites por arquivo, lote, páginas, texto, CPU, memória, duração e disco de saída. Sem truncamento silencioso.
- Parser SIAFI orientado a seções: CNPJ do favorecido separado do CNPJ emitente; valor do cabeçalho separado dos itens; número do empenho reconstruído por ano/tipo/número. Gestão ausente continua ausente. Pregão/SRP não significa adesão à ata; inferir carona só com evidência explícita. Campos ausentes viram avisos editáveis.
- Interface HTML/JS sem framework nem build Node; upload múltiplo com concorrência limitada no cliente, conferência antes de salvar, cadastro manual de NF, cálculo e download de minuta.
- SQLite mantém as tabelas existentes; ler/gravar valores em centavos no domínio, com conversão explícita das colunas REAL antigas. BEGIN IMMEDIATE para limite das NFs. Sessão demonstrativa permanece somente em loopback; token obrigatório se o bind for externo. Empresa vem da configuração do servidor, nunca de identificador enviado pelo navegador. Não apresentar isso como um SaaS com cadastro/login de múltiplas empresas.
- Remover o runtime Python depois de testes Rust e backup local dos arquivos substituídos; preservar PDFs e banco existente. Entregar Dockerfile e unidade systemd para revisão; não instalá-los.

## Execução e verificação

1. Fixar resultados esperados dos 7 PDFs e escrever testes de dinheiro, extração, datas e classificação antes da implementação.
2. Implementar domínio/parser e pipeline limitado; verificar os 7 documentos e entradas corrompidas, grandes, protegidas/sem texto e incompletas.
3. Migrar persistência e HTTP/interface; testar isolamento, CNPJ, saldo concorrente, compatibilidade SQLite e fluxo no navegador.
4. Executar testes, fmt, Clippy, release build, revisão Rust/segurança e teste de uploads simultâneos com afinidade de 2 CPUs. Medir RSS do servidor e dos subprocessos; distinguir medição local de validação real da VPS.
5. Documentar limites, dependências Poppler/util-linux, uso do token e resultados medidos. Nenhuma chamada externa de IA.

## Execução concluída

- [x] Parser e centavos testados, incluindo os 7 empenhos reais.
- [x] Pipeline com limites de processo, arquivos, filas e concorrência, com testes de entradas inválidas.
- [x] SQLite/HTTP/interface migrados; legado e saldo concorrente testados; regressão de seleção corrigida.
- [x] 19 testes Rust e 2 JS, Clippy/fmt, cobertura 80,04%, navegador e build/runtime Docker verificados.
- [x] Carga: 140 análises, 16 clientes, afinidade de 2 CPUs; resultados e limitações em `docs/validacao-rust.md`.
- [x] Arquivos Python substituídos com backup local; banco e corpus preservados; nenhum deploy/commit/push.
