"use strict";
const fs = require("node:fs");
const vm = require("node:vm");
const assert = require("node:assert/strict");
const test = require("node:test");

const source = fs.readFileSync("web/app.js", "utf8");
const start = source.indexOf("async function atualizar()");
const end = source.indexOf("async function conectar()", start);

async function atualizarComSelecao(itens) {
  const select = {
    children: [{value:"11",textContent:"A"},{value:"22",textContent:"B"}], value:"22",
    get selectedOptions() { return this.children.filter(item => item.value === this.value); },
    get options() { return this.children; },
    replaceChildren() { this.children=[]; this.value=""; },
    append(option) { this.children.push(option); if(this.children.length===1)this.value=String(option.value); }
  };
  const context = {
    json:async path=>path.startsWith("/empenhos") ? {itens} : {itens:[]},
    $:id=>id==="empenho-select" ? select : {replaceChildren(){}},
    empOffset:0, nfOffset:100, linha(){}, paginar(){}, moeda(){return "500,00";},
    document:{createElement(){return {};}}
  };
  vm.createContext(context);
  vm.runInContext(source.slice(start,end),context);
  await context.atualizar();
  return select.value;
}

const a={id:11,empenho:"A",ug:"000001",modalidade:"",valor_total_centavos:50000};
const b={...a,id:22,empenho:"B"};

test("paginar notas preserva o empenho escolhido para a NF",async()=>{
  assert.equal(await atualizarComSelecao([a,b]),"22");
});
test("paginar empenhos também preserva a escolha anterior da NF",async()=>{
  assert.equal(await atualizarComSelecao([a]),"22");
});
