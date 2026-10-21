/**
 * Tipos do contrato da API v1 (docs/saas/03-api-v1.md).
 *
 * Convenções do contrato:
 * - Datas de calendário: `Data` = "YYYY-MM-DD". Instantes: `Instante` = ISO-8601 UTC.
 * - Dinheiro em centavos (`*_centavos`, inteiro). Taxas em pontos-base (`*_bps`, 100 = 1%).
 * - IDs são UUID (`Id`). Limite de plano ilimitado = `null`.
 *
 * Mudou o contrato? Atualize aqui e avise os agentes que consomem estes tipos.
 */

export type Id = string
/** Data de calendário "YYYY-MM-DD". */
export type Data = string
/** Instante ISO-8601 UTC. */
export type Instante = string
export type Centavos = number
export type Bps = number

// ---------------------------------------------------------------------------
// Envelope e erros (ADR-009)
// ---------------------------------------------------------------------------

export interface Meta {
  total: number
  pagina: number
  por_pagina: number
  /** Só em `/notificacoes`. */
  nao_lidas?: number
}

export interface Envelope<T> {
  dados: T
  meta?: Meta
}

/** Lista paginada já desembrulhada pelo cliente (`api.pagina`). */
export interface Pagina<T> {
  dados: T[]
  meta: Meta
}

export type CodigoErro =
  | 'nao_autenticado'
  | 'credenciais_invalidas'
  | 'proibido'
  | 'nao_encontrado'
  | 'validacao'
  | 'conflito'
  | 'limite_plano'
  | 'ocupado'
  | 'interno'
  | 'indisponivel'
  | 'rede'
  | (string & {})

export interface CorpoErro {
  erro: {
    codigo: CodigoErro
    mensagem: string
    campos?: Record<string, string>
  }
}

export interface ParametrosPagina {
  pagina?: number
  por_pagina?: number
}

// ---------------------------------------------------------------------------
// Identidade e organização
// ---------------------------------------------------------------------------

export type Papel = 'dono' | 'admin' | 'operador' | 'leitor'
export type Plano = 'gratuito' | 'profissional' | 'empresarial'
export type StatusAssinatura = 'teste' | 'ativa' | 'inadimplente' | 'cancelada'

export interface Usuario {
  id: Id
  nome: string
  email: string
}

/** Organização como aparece em `Me.organizacoes` e em `POST /orgs`. */
export interface OrganizacaoResumo {
  id: Id
  nome: string
  papel: Papel
  plano: Plano
}

export interface Me {
  usuario: Usuario
  organizacoes: OrganizacaoResumo[]
}

export interface Limites {
  cnpjs: number | null
  empenhos_ativos: number | null
  usuarios: number | null
  transparencia: boolean
  alertas_email: boolean
}

export interface Uso {
  cnpjs: number
  empenhos_ativos: number
  usuarios: number
}

export interface Assinatura {
  plano: Plano
  status: StatusAssinatura
  teste_ate: Instante | null
  /** Provedor de cobrança; hoje sempre "manual" (ADR-013). */
  provedor?: string
  /**
   * Troca para plano pago disponível (`FELIX_PLANO_MANUAL`). Ausente = `true`
   * (servidores antigos). Com `false`, só é possível mudar para o Gratuito.
   */
  troca_paga_disponivel?: boolean
  limites: Limites
  uso: Uso
}

/** `GET /orgs/{org}`. */
export interface Organizacao {
  id: Id
  nome: string
  papel: Papel
  assinatura: Assinatura
}

export interface Membro {
  usuario_id: Id
  nome: string
  email: string
  papel: Papel
  desde: Instante
}

export interface Convite {
  id: Id
  email: string
  papel: Papel
  expira_em: Instante
  criado_em: Instante
}

/** `GET /convites/{token}` (público). */
export interface ConvitePublico {
  organizacao_nome: string
  email: string
  papel: Papel
  usuario_existe: boolean
}

/** `POST /convites/{token}/aceitar`: o `Me` atualizado e, quando enviado, a organização do convite. */
export interface ConviteAceito extends Me {
  organizacao_id?: Id
}

export interface EntradaAuditoria {
  id: number | string
  usuario_id?: Id | null
  usuario_nome: string | null
  acao: string
  entidade: string
  entidade_id: string | null
  dados: unknown
  criado_em: Instante
}

export interface EmailDev {
  id: Id
  para: string
  assunto: string
  corpo: string
  status: 'registrado' | 'enviado' | 'falhou'
  criado_em: Instante
}

// Corpos de requisição -------------------------------------------------------

export interface CadastroEntrada {
  nome: string
  email: string
  senha: string
  organizacao_nome: string
}

export interface LoginEntrada {
  email: string
  senha: string
}

export interface RedefinirSenhaEntrada {
  token: string
  senha: string
}

export interface AceitarConviteEntrada {
  nome?: string
  senha?: string
}

export interface TrocarSenhaEntrada {
  senha_atual: string
  senha_nova: string
}

export interface NovoConvite {
  email: string
  papel: Papel
}

// ---------------------------------------------------------------------------
// Empresas (CNPJs)
// ---------------------------------------------------------------------------

export interface Empresa {
  id: Id
  razao_social: string
  /** 14 dígitos, sem máscara. */
  cnpj: string
  criado_em: Instante
}

export interface NovaEmpresa {
  razao_social: string
  cnpj: string
}

// ---------------------------------------------------------------------------
// Empenhos
// ---------------------------------------------------------------------------

export type RegraVencimento = 'corridos_emissao' | 'in77_uteis'
export type OrigemEmpenho = 'pdf' | 'manual' | 'transparencia' | 'legado'
export type StatusEmpenho = 'ativo' | 'anulado'

export interface NovoEmpenho {
  empresa_id: Id
  ug: string
  ug_nome?: string | null
  orgao?: string | null
  gestao?: string | null
  numero: string
  modalidade?: string | null
  numero_edital_ata?: string | null
  valor_total_centavos: Centavos
  data_emissao?: Data | null
  regra_vencimento?: RegraVencimento
  prazo_dias?: number
  origem?: 'pdf' | 'manual'
  observacao?: string | null
}

/** `PATCH /empenhos/{id}`: campos de `NovoEmpenho` exceto `empresa_id`. */
export type EdicaoEmpenho = Partial<Omit<NovoEmpenho, 'empresa_id'>>

export interface FiltrosEmpenhos extends ParametrosPagina {
  busca?: string
  empresa_id?: Id
  ug?: string
  /** Padrão no servidor: `ativo`. `todos` inclui anulados. */
  status?: StatusEmpenho | 'todos'
  com_saldo?: boolean
}

export interface EmpenhoResumo {
  id: Id
  empresa_id: Id
  empresa_cnpj: string
  ug: string
  ug_nome: string | null
  orgao: string | null
  numero: string
  modalidade: string | null
  valor_total_centavos: Centavos
  faturado_centavos: Centavos
  recebido_centavos: Centavos
  saldo_centavos: Centavos
  status: StatusEmpenho
  origem: OrigemEmpenho
  data_emissao: Data | null
  criado_em: Instante
}

export interface Empenho extends EmpenhoResumo {
  gestao: string | null
  numero_edital_ata: string | null
  regra_vencimento: RegraVencimento
  prazo_dias: number
  observacao: string | null
  /** Preenchidos quando `status = anulado`. */
  anulado_em?: Instante | null
  motivo_anulacao?: string | null
  notas?: Nota[]
}

/** Dados extraídos do PDF SIAFI (felix-core `DadosEmpenho`). */
export interface DadosEmpenhoExtraidos {
  ug: string | null
  gestao: string | null
  /** Número no formato "2026NE000123". */
  empenho: string | null
  cnpj_fornecedor: string | null
  cnpj_emitente: string | null
  fornecedor: string | null
  valor_total_centavos: Centavos | null
  data_emissao: Data | null
  modalidade: string | null
  /** "licitacao" | "contratacao_direta" | "adesao_ata" | "nao_identificada". */
  origem: string
  numero_edital_ata: string | null
}

export interface AnaliseEmpenho {
  /** "siafi" | "desconhecido". */
  perfil: string
  dados: DadosEmpenhoExtraidos
  avisos: string[]
}

/** Item de `POST /empenhos/analisar`. */
export interface ResultadoAnalise {
  arquivo: string
  analise?: AnaliseEmpenho
  erro?: string
  empresa_id_sugerida?: Id | null
  ja_cadastrado: boolean
}

export interface AnularEmpenhoEntrada {
  motivo: string
}

// ---------------------------------------------------------------------------
// Notas fiscais
// ---------------------------------------------------------------------------

export type TipoNota = 'produto' | 'servico'
export type StatusNota = 'emitida' | 'liquidada' | 'paga' | 'cancelada'
export type SituacaoNota = 'vencida' | 'a_vencer' | 'vence_7d'
export type TipoEvento = 'liquidada' | 'paga' | 'cancelada' | 'reaberta'
export type OrigemEvento = 'manual' | 'transparencia'

export interface Nota {
  id: Id
  empenho_id: Id
  empenho_numero: string
  ug: string
  ug_nome: string | null
  empresa_cnpj: string
  numero: string
  tipo: TipoNota
  valor_centavos: Centavos
  data_emissao: Data
  data_recebimento: Data | null
  vencimento: Data
  status: StatusNota
  dias_atraso: number
  valor_pago_centavos: Centavos
  data_liquidacao: Data | null
  data_pagamento: Data | null
  documento_liquidacao: string | null
  documento_pagamento: string | null
  criado_em: Instante
}

/** `GET /notas/{id}`. */
export interface NotaDetalhe extends Nota {
  eventos: Evento[]
  cobrancas: CobrancaResumo[]
}

export interface NovaNota {
  numero: string
  tipo: TipoNota
  valor_centavos: Centavos
  data_emissao: Data
  data_recebimento?: Data | null
}

export interface EdicaoNota {
  numero?: string
  valor_centavos?: Centavos
  data_emissao?: Data
  data_recebimento?: Data | null
}

export interface FiltrosNotas extends ParametrosPagina {
  /** Um ou mais status; o cliente envia separados por vírgula. */
  status?: StatusNota[]
  empenho_id?: Id
  situacao?: SituacaoNota
  busca?: string
  ordenar?: 'vencimento' | (string & {})
}

export interface Evento {
  id: Id
  tipo: TipoEvento
  data: Data
  valor_centavos: Centavos | null
  documento: string | null
  observacao: string | null
  origem: OrigemEvento
  criado_por_nome: string | null
  criado_em: Instante
}

export interface NovoEvento {
  tipo: TipoEvento
  data: Data
  valor_centavos?: Centavos | null
  documento?: string | null
  observacao?: string | null
}

// ---------------------------------------------------------------------------
// Cobranças
// ---------------------------------------------------------------------------

export type CanalCobranca = 'email' | 'protocolo' | 'sei' | 'correios' | 'outro'

export interface Calculo {
  vencimento: Data
  dias_atraso: number
  principal_centavos: Centavos
  multa_centavos: Centavos
  juros_centavos: Centavos
  total_centavos: Centavos
}

export interface ParametrosCalculo {
  multa_bps?: Bps
  juros_bps?: Bps
  data_referencia?: Data
}

export interface CobrancaResumo {
  id: Id
  nota_id: Id
  nota_numero: string
  empenho_numero: string
  ug: string
  canal: CanalCobranca
  protocolo: string | null
  enviada_em: Data
  total_centavos: Centavos
  dias_atraso: number
  criado_por_nome: string | null
  criado_em: Instante
}

export interface Cobranca extends CobrancaResumo {
  multa_bps: Bps
  juros_bps: Bps
  data_referencia?: Data
  principal_centavos: Centavos
  multa_centavos: Centavos
  juros_centavos: Centavos
  observacao: string | null
  texto: string
}

export interface NovaCobranca {
  multa_bps: Bps
  juros_bps: Bps
  canal: CanalCobranca
  protocolo?: string | null
  enviada_em: Data
  observacao?: string | null
}

export interface FiltrosCobrancas extends ParametrosPagina {
  nota_id?: Id
}

// ---------------------------------------------------------------------------
// Painel
// ---------------------------------------------------------------------------

export type FaixaAging = 'a_vencer' | '1_30' | '31_60' | '61_90' | '90_mais'

export interface ItemAging {
  faixa: FaixaAging
  centavos: Centavos
  quantidade: number
}

export interface ItemPorUg {
  ug: string
  ug_nome: string | null
  aberto_centavos: Centavos
  vencido_centavos: Centavos
  atraso_medio_dias: number
}

export interface Painel {
  a_receber_centavos: Centavos
  vencido_centavos: Centavos
  vence_30d_centavos: Centavos
  recebido_mes_centavos: Centavos
  notas_abertas: number
  notas_vencidas: number
  empenhos_ativos: number
  saldo_empenhos_centavos: Centavos
  aging: ItemAging[]
  por_ug: ItemPorUg[]
  proximos_vencimentos: Nota[]
  mais_atrasadas: Nota[]
}

// ---------------------------------------------------------------------------
// Notificações
// ---------------------------------------------------------------------------

export interface Notificacao {
  id: Id
  /** Ex.: "vence_5d", "vencida". */
  tipo: string
  titulo: string
  corpo: string
  link: string | null
  lida_em: Instante | null
  criado_em: Instante
}

export interface ResultadoAlertas {
  criadas: number
}

// ---------------------------------------------------------------------------
// Integração Portal da Transparência
// ---------------------------------------------------------------------------

export interface IntegracaoStatus {
  configurada_no_servidor: boolean
  permitida_no_plano: boolean
  ativa: boolean
  ultima_sincronizacao_em: Instante | null
  ultimo_status: string | null
  ultimo_erro: string | null
  pendentes: number
  /** Há sincronização pendente ou em execução (o servidor envia; fora do contrato mínimo). */
  sincronizando?: boolean
}

export interface SincronizacaoIniciada {
  job_id: Id
}

/** 1 = empenho, 2 = liquidação, 3 = pagamento. */
export type FaseTransparencia = 1 | 2 | 3
export type SituacaoDocumento = 'novo' | 'vinculado' | 'ignorado'

export interface SugestaoVinculo {
  tipo: 'empenho' | 'nota'
  id: Id
  rotulo: string
}

export interface DocumentoTransparencia {
  id: Id
  empresa_cnpj: string
  codigo: string
  codigo_resumido: string | null
  fase: FaseTransparencia
  /** Pode faltar no Portal. */
  data: Data | null
  /** Pode faltar no Portal. */
  valor_centavos: Centavos | null
  ug: string | null
  ug_nome: string | null
  orgao: string | null
  observacao: string | null
  empenho_codigo: string | null
  situacao: SituacaoDocumento
  sugestao?: SugestaoVinculo | null
  /** Campos extras enviados pelo servidor. */
  empresa_id?: Id
  especie?: string | null
  empenho_id?: Id | null
  nota_id?: Id | null
  importado_em?: Instante
}

export interface FiltrosDocumentos extends ParametrosPagina {
  situacao?: SituacaoDocumento
  fase?: FaseTransparencia
}

export type VinculoDocumento = { empenho_id: Id } | { nota_id: Id }
