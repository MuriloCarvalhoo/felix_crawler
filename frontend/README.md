# Felix: frontend

SPA em Vite + React 19 + TypeScript estrito, TanStack Router (rotas por arquivo), Query, Table v9 e Form, Zod e Tailwind CSS v4 (ADR-002). Em produção o binário Rust serve `frontend/dist` na mesma origem da API (ADR-016).

## Comandos

```bash
npm install
npm run dev        # http://127.0.0.1:5179, proxy de /api e /health para 127.0.0.1:8090
npm test           # Vitest (jsdom)
npm run lint       # ESLint (typescript-eslint strict) + tsc --noEmit
npm run build      # tsc -b && vite build → dist/
npm run rotas      # regenera src/routeTree.gen.ts (também roda antes de build e lint)
```

`FELIX_API=http://127.0.0.1:8099 npm run dev` aponta o proxy para outro backend, como um mock local. O proxy mantém `changeOrigin: false` para que Origin e Host batam na verificação de CSRF.

## Estrutura

```text
src/
  main.tsx                 providers (Query, Toast, Router) e tratador global de 401
  router.tsx               router + queryClient singletons
  styles.css               tokens de cor/tipo (claro e escuro) e @theme do Tailwind
  routeTree.gen.ts         gerado; não editar
  lib/
    api.ts                 cliente HTTP (get, pagina, post, patch, put, del, upload, baixar, salvarArquivo, rotaOrg)
    tipos.ts               todos os tipos do contrato docs/saas/03-api-v1.md
    consultas.ts           chaves de consulta, opcoesMe, useMe, useOrgAtual, limparSessao
    permissoes.ts          pode(papel, acao) conforme a ADR-008; NOMES_PAPEL
    formatos.ts            BRL, datas, CNPJ, UG, bps, atraso
    guardas.ts             exigirSessao / sessaoOpcional para beforeLoad
    navegacao.ts           ehRotaProtegida, destinoSeguro (anti open-redirect)
    tema.ts, cx.ts, queryClient.ts
  components/
    ui/                    componentes base (index.ts exporta tudo)
    form/formulario.tsx    useAppForm + campos prontos + aplicarErrosServidor
    layout/                AppShell, AuthLayout, LayoutSimples, SeletorOrg, MenuUsuario, EmConstrucao
  features/<area>/         código de cada área (componentes, hooks, esquemas zod)
  routes/                  rotas por arquivo (ver tabela)
```

## Rotas e donos

| URL | Arquivo | Dono |
|---|---|---|
| `/entrar`, `/cadastrar`, `/esqueci-senha`, `/redefinir-senha?token=`, `/convite/$token` | `routes/*.tsx`, `routes/convite/$token.tsx` | D |
| `/` | `routes/index.tsx` (redireciona) | D |
| `/conta` | `routes/conta.tsx` | H |
| `/o/$orgId` (layout + guarda) | `routes/o/$orgId/route.tsx` | D |
| `/o/$orgId` (painel) | `routes/o/$orgId/index.tsx` | I |
| `/o/$orgId/empenhos`, `/novo`, `/$empenhoId` | `routes/o/$orgId/empenhos/*` | I |
| `/o/$orgId/notas`, `/$notaId` | `routes/o/$orgId/notas/*` | I |
| `/o/$orgId/cobrancas` | `routes/o/$orgId/cobrancas/index.tsx` | I |
| `/o/$orgId/integracoes`, `/notificacoes` | `routes/o/$orgId/{integracoes,notificacoes}/index.tsx` | K |
| `/o/$orgId/configuracoes/{organizacao,empresas,equipe,plano,auditoria}` | `routes/o/$orgId/configuracoes/*` (o `route.tsx` tem as abas) | H |

O sino da barra superior vem de `features/notificacoes/SinoNotificacoes.tsx` (placeholder; o agente K substitui).

## Convenções

- **Textos** em português do Brasil, com acentos e em sentence case. Botões dizem a ação ("Salvar empenho", não "Enviar"). Erros explicam o que houve e como resolver.
- **API:** sempre via `@/lib/api`. Caminhos são relativos a `/api/v1`. Para recursos da organização use `rotaOrg(org, '/empenhos')`. Listas paginadas usam `api.pagina<T>()`, que devolve `{dados, meta}`. Mutações já mandam `X-Felix: 1`. Erros chegam como `ErroApi {status, codigo, mensagem, campos}`.
- **Dinheiro** é sempre inteiro em centavos. Leia com `lerBRL`, mostre com `<Valor centavos />` ou `formatarBRL`. Datas de calendário são texto `YYYY-MM-DD`: use `formatarData` e `hojeISO`, nunca `new Date('2026-10-20')`.
- **Consultas:** chaves vêm de `chaves.*` em `@/lib/consultas` e todas começam com `['org', orgId]`. Depois de uma mutação, invalide o grupo: `qc.invalidateQueries({ queryKey: chaves.empenhosTodos(org) })`, ou `invalidarOrg(qc, org)`. Chaves novas de feature podem ficar no próprio `features/<area>/consultas.ts`, usando `chaveOrg(org, 'minha-area', filtros)`.
- **Organização atual:** dentro de `/o/$orgId/...` use `const org = useOrgAtual()`, que devolve `{id, nome, papel, plano, pode(acao)}`. Esconda os controles com `org.pode('criar_empenho')`; o servidor continua sendo quem autoriza.
- **Formulários:** `useAppForm` + `validators: { onChange: esquemaZod }`. Use os campos `CampoTexto`, `CampoSenha`, `CampoAreaTexto`, `CampoSelecao`, `CampoDinheiro`, `CampoData`, `CampoCNPJ` e `CampoCaixa`, e `<form.AppForm><form.BotaoEnviar>`. No `catch` do `onSubmit`, chame `setErroGeral(aplicarErrosServidor(formApi, erro))` e mostre `<ErroFormulario mensagem={erroGeral} />`. Ver `features/auth/FormEntrar.tsx`.
- **Tabelas:** `const col = criarColunas<Tipo>()` fora do componente, `col.columns([...])` e `<DataTable legenda colunas dados carregando erro vazio />`. Use `meta: { alinhar: 'fim' }` para valores e `ocultarNoCelular` para colunas secundárias. Para paginação do servidor, combine com `<Pagination pagina porPagina total aoMudarPagina />`.
- **Estados:** carregando (`Carregando`, `Skeleton`, `DataTable carregando`), vazio (`EmptyState` com a próxima ação) e erro (`ErrorState erro aoTentarDeNovo`).
- **Avisos:** `const toast = useToast(); toast.sucesso('Empenho salvo')` ou `toast.erro(erro)`.
- **Estilo:** use só os tokens (`bg-superficie`, `text-tinta-2`, `border-linha`, `text-lacre`, `bg-tesouro`…), que trocam sozinhos no tema escuro. Não use `dark:`. Use `cifras` para colunas numéricas. Por causa da CSP (`default-src 'self'`), não use scripts inline nem CDNs: a fonte já vem do pacote `@fontsource-variable/public-sans`.
- **Acessibilidade:** cada página tem um único `<h1>` (via `PageHeader`). Botões só com ícone levam `aria-label`. Campos usam `Field` ou os campos do kit, que já ligam rótulo, ajuda e erro. O layout deve funcionar a partir de 390 px de largura.

## Como criar uma página de feature

1. Crie ou substitua o arquivo em `src/routes/o/$orgId/<area>/...` com `createFileRoute('/o/$orgId/<area>/')`. O `npm run dev` regenera a árvore de rotas.
2. Ponha componentes, esquemas e hooks em `src/features/<area>/`.
3. Na página, chame `useOrgAtual()` e `useQuery({ queryKey: chaves.x(org.id, filtros), queryFn: () => api.pagina<T>(rotaOrg(org.id, '/x'), { query: filtros }) })`.
4. Renderize `<PageHeader titulo acoes />` e depois o conteúdo.
5. Para filtros na URL, declare `validateSearch: z.object({...})` na rota.
6. Escreva os testes em `*.test.tsx`, ao lado do código.
