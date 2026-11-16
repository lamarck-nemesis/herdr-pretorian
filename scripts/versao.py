#!/usr/bin/env python3
"""Versionamento semântico do repositório: VERSION é a fonte da verdade e o CHANGELOG registra cada versão.

uso:
  python scripts/versao.py                      mostra a versão atual
  python scripts/versao.py verificar            confere VERSION, arquivos sincronizados e CHANGELOG
  python scripts/versao.py lancar patch|minor|major|X.Y.Z [--data AAAA-MM-DD]

`lancar` move as entradas de "## [Não lançado]" para a nova versão, grava VERSION e os
arquivos sincronizados e mostra os comandos de commit e tag; não executa git.
"""
import argparse
import datetime
import json
import re
import sys
from pathlib import Path

# --- configuração deste repositório -------------------------------------------------
CHANGELOG = 'PRETORIAN-CHANGELOG.md'
PREFIXO_TAG = 'pretorian-v'  # as tags v* pertencem ao Herdr upstream
# Arquivos JSON cuja versão acompanha VERSION: (caminho, [chaves até o campo "version"]).
SINCRONIZADOS = []  # Cargo.toml mantém a versão do Herdr upstream; o binário lê VERSION
# -------------------------------------------------------------------------------------

RAIZ = Path(__file__).resolve().parents[1]
SEMVER = re.compile(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?')
NAO_LANCADO = '## [Não lançado]'
SECAO = re.compile(r'^## \[([^\]]+)\](?: - (\d{4}-\d{2}-\d{2}))?\s*$', re.M)


class ErroVersao(Exception):
    pass


def ler_versao():
    versao = (RAIZ / 'VERSION').read_text(encoding='utf-8').strip()
    if not SEMVER.fullmatch(versao):
        raise ErroVersao(f'VERSION inválido: {versao!r}; use X.Y.Z (semver).')
    return versao


def proxima(atual, pedido):
    if SEMVER.fullmatch(pedido):
        return pedido
    major, minor, patch = (int(p) for p in SEMVER.fullmatch(atual).groups()[:3])
    saltos = {'major': (major + 1, 0, 0), 'minor': (major, minor + 1, 0), 'patch': (major, minor, patch + 1)}
    if pedido not in saltos:
        raise ErroVersao('Informe patch, minor, major ou uma versão X.Y.Z.')
    return '.'.join(map(str, saltos[pedido]))


def chave(versao):
    major, minor, patch, pre = SEMVER.fullmatch(versao).groups()
    # Sem pré-lançamento vem depois do pré-lançamento da mesma versão.
    return (int(major), int(minor), int(patch), pre is None, pre or '')


def secoes(texto):
    return [(m.group(1), m.group(2), m.start(), m.end()) for m in SECAO.finditer(texto)]


def campo_json(dados, caminho):
    for parte in caminho:
        dados = dados[parte]
    return dados


def verificar():
    versao = ler_versao()
    problemas = []
    for arquivo, caminho in SINCRONIZADOS:
        valor = campo_json(json.loads((RAIZ / arquivo).read_text(encoding='utf-8')), caminho)
        if valor != versao:
            problemas.append(f'{arquivo} ({".".join(caminho)}) tem {valor}, esperado {versao}.')
    texto = (RAIZ / CHANGELOG).read_text(encoding='utf-8')
    lista = secoes(texto)
    if not lista or lista[0][0] != 'Não lançado':
        problemas.append(f'{CHANGELOG} deve começar pela seção "{NAO_LANCADO}".')
    lancadas = [(nome, data) for nome, data, *_ in lista if nome != 'Não lançado']
    for nome, data in lancadas:
        if not SEMVER.fullmatch(nome) or not data:
            problemas.append(f'{CHANGELOG}: seção "{nome}" deve ser "## [X.Y.Z] - AAAA-MM-DD".')
    nomes = [nome for nome, _ in lancadas if SEMVER.fullmatch(nome)]
    if nomes != sorted(nomes, key=chave, reverse=True) or len(set(nomes)) != len(nomes):
        problemas.append(f'{CHANGELOG}: versões fora de ordem decrescente ou repetidas.')
    if not nomes or nomes[0] != versao:
        problemas.append(f'{CHANGELOG}: a versão mais recente registrada deve ser {versao} (VERSION).')
    if problemas:
        raise ErroVersao('\n'.join(problemas))
    return versao


def lancar(pedido, data):
    atual = verificar()
    nova = proxima(atual, pedido)
    if chave(nova) <= chave(atual):
        raise ErroVersao(f'A nova versão ({nova}) deve ser maior que a atual ({atual}).')
    datetime.date.fromisoformat(data)
    caminho = RAIZ / CHANGELOG
    texto = caminho.read_text(encoding='utf-8')
    lista = secoes(texto)
    inicio_corpo, fim_corpo = lista[0][3], lista[1][2] if len(lista) > 1 else len(texto)
    corpo = texto[inicio_corpo:fim_corpo].strip()
    if not re.search(r'^- ', corpo, re.M):
        raise ErroVersao(f'A seção "{NAO_LANCADO}" está vazia; registre as mudanças antes de lançar.')
    novo = texto[:lista[0][2]] + f'{NAO_LANCADO}\n\n## [{nova}] - {data}\n\n{corpo}\n\n' + texto[fim_corpo:].lstrip('\n')
    caminho.write_text(novo, encoding='utf-8')
    (RAIZ / 'VERSION').write_text(nova + '\n', encoding='utf-8')
    for arquivo, chaves in SINCRONIZADOS:
        alvo = RAIZ / arquivo
        dados = json.loads(alvo.read_text(encoding='utf-8'))
        campo_json(dados, chaves[:-1])[chaves[-1]] = nova
        alvo.write_text(json.dumps(dados, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    arquivos = ' '.join(['VERSION', CHANGELOG, *dict.fromkeys(a for a, _ in SINCRONIZADOS)])
    print(f'{atual} → {nova}. Revise e conclua:\n'
          f'  git add {arquivos}\n'
          f'  git commit -m "release: {PREFIXO_TAG}{nova}"\n'
          f'  # após o merge na branch principal, no commit do merge:\n'
          f'  git tag -a {PREFIXO_TAG}{nova} -m "{PREFIXO_TAG}{nova}" && git push origin {PREFIXO_TAG}{nova}')
    return nova


def main(argv=None):
    parser = argparse.ArgumentParser(description='Versionamento semântico do repositório')
    sub = parser.add_subparsers(dest='comando')
    sub.add_parser('verificar', help='confere VERSION, arquivos sincronizados e CHANGELOG')
    lanca = sub.add_parser('lancar', help='fecha a seção Não lançado em uma nova versão')
    lanca.add_argument('nivel', help='patch, minor, major ou X.Y.Z')
    lanca.add_argument('--data', default=datetime.date.today().isoformat(), help='data da versão (AAAA-MM-DD)')
    args = parser.parse_args(argv)
    try:
        if args.comando == 'verificar':
            print(f'{verificar()}: VERSION, arquivos sincronizados e {CHANGELOG} consistentes.')
        elif args.comando == 'lancar':
            lancar(args.nivel, args.data)
        else:
            print(ler_versao())
    except (ErroVersao, ValueError, OSError, KeyError) as erro:
        print(f'erro: {erro}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
