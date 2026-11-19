#!/usr/bin/env python3
"""Check the prepared Mason registry submission against this repository.

PyYAML is not available, so `clients/mason/package.yaml` is read line by line.
The prepared package must stay in step with Cargo.toml and the release archive
contract: same version in the purl and the asset filename, the Cargo SPDX
license, the `linux_x64_gnu` platform key and a `bin` path for an archive with a
top-level directory. Run by `make mason-package-check`.
"""

import re
import sys
from pathlib import Path

PACKAGE = Path('clients/mason/package.yaml')
CARGO_MANIFEST = Path('Cargo.toml')
OWNER_REPO = 'BlueDrink9/magit-lsp'
HOMEPAGE = 'https://github.com/BlueDrink9/magit-lsp'
TARGET = 'linux_x64_gnu'
TARGET_TRIPLE = 'x86_64-unknown-linux-gnu'
CATEGORIES = ('Compiler', 'DAP', 'Formatter', 'LSP', 'Linter', 'Runtime')
REQUIRED_KEYS = (
    'name',
    'description',
    'homepage',
    'licenses',
    'languages',
    'categories',
    'source',
)
ASSET_PATTERN = re.compile(
    r'magit-lsp-v(?P<version>(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))'
    r'-' + re.escape(TARGET_TRIPLE) + r'\.tar\.gz'
)
PURL_PATTERN = re.compile(
    r'pkg:github/(?P<owner>[^/@\s]+)/(?P<repo>[^/@\s]+)@v'
    r'(?P<version>(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))'
)


class CheckError(Exception):
    """One failed check, reported without a traceback."""


def fail(reason):
    raise CheckError(reason)


def read_lines(path):
    try:
        return path.read_text(encoding='utf-8').splitlines()
    except OSError as error:
        fail(f'cannot read {path}: {error}')


def strip_comment(line):
    """Drop a YAML comment; no quoted value in this package contains `#`."""
    for index, character in enumerate(line):
        if character == '#' and (index == 0 or line[index - 1] in ' \t'):
            return line[:index]
    return line


def content_lines(lines):
    """Return (indent, text) for non-blank, non-comment lines."""
    content = []
    for number, line in enumerate(lines, start=1):
        text = strip_comment(line).rstrip()
        if not text.strip():
            continue
        content.append((len(text) - len(text.lstrip(' ')), text.lstrip(' '), number))
    return content


def parse_top_level(content):
    top = {}
    for indent, text, number in content:
        if indent == 0:
            if ':' not in text:
                fail(f'line {number}: expected `key: value`, got {text!r}')
            key, _, value = text.partition(':')
            key = key.strip()
            if key in top:
                fail(f'line {number}: duplicate top-level key {key!r}')
            top[key] = {'value': value.strip(), 'children': [], 'line': number}
        else:
            if not top:
                fail(f'line {number}: indented line before any top-level key')
            current = list(top)[-1]
            top[current]['children'].append((indent, text, number))
    return top


def parse_asset(children):
    fields = {}
    assets = []
    current = None
    for indent, text, number in children:
        key, separator, value = text.partition(':')
        if not separator:
            fail(f'line {number}: expected `key: value` inside source, got {text!r}')
        key = key.strip()
        value = value.strip()
        if key == 'asset' and not value:
            current = None
        elif text.startswith('- '):
            entry = {}
            assets.append(entry)
            key, _, value = text[2:].strip().partition(':')
            entry[key.strip()] = value.strip()
            current = entry
        elif current is not None and indent >= 6:
            current[key] = value
        else:
            fields[key] = value
            current = None
    return fields, assets


def mapping(holder):
    """Parse a small `name: value` mapping from a key's indented children."""
    result = {}
    for indent, text, number in holder['children']:
        key, separator, value = text.partition(':')
        if not separator or indent < 2:
            fail(f'line {number}: expected `name: value` under {holder["line"]}')
        result[key.strip()] = unquote(value.strip())
    return result


def list_values(holder, key):
    """Parse `- value` children, accepting the inline empty list."""
    value = holder['value']
    if value in ('[]', '[ ]'):
        return []
    if value:
        fail(f'{key}: expected a list, got inline value {value!r}')
    result = []
    for indent, text, number in holder['children']:
        if not text.startswith('- '):
            fail(f'line {number}: expected a `- value` list entry under {key}')
        item = text[2:].strip()
        if item:
            result.append(item)
    return result


def unquote(value):
    if len(value) >= 2 and value[0] == value[-1] and value[0] in '"\'':
        return value[1:-1]
    return value


def cargo_metadata():
    lines = read_lines(CARGO_MANIFEST)

    def field(name):
        pattern = re.compile(rf'^{re.escape(name)}\s*=\s*"([^"]+)"')
        for line in lines:
            match = pattern.match(line)
            if match:
                return match.group(1)
        fail(f'Cargo.toml: package.{name} is missing')

    return {
        'version': field('version'),
        'license': field('license'),
        'description': field('description'),
        'repository': field('repository'),
    }


def check():
    cargo = cargo_metadata()
    lines = read_lines(PACKAGE)
    content = content_lines(lines)
    if not content:
        fail(f'{PACKAGE} is empty')
    if content[0][1] != '---':
        fail(f'{PACKAGE}: first content line must be `---`, got {content[0][1]!r}')
    top = parse_top_level(content[1:])

    for key in REQUIRED_KEYS:
        if key not in top:
            fail(f'{PACKAGE}: required top-level key {key!r} is missing')
    if 'install' in top:
        fail(f'{PACKAGE}: an `install:` stanza is not part of the registry schema')
    for _, text, number in content:
        if 'supported_platforms' in text:
            fail(f'{number}: GitHub-release sources must not set supported_platforms')
        if 'checksum' in text:
            fail(f'{number}: the registry schema has no checksum field')

    for key in ('name', 'description', 'homepage'):
        if not top[key]['value']:
            fail(f'{PACKAGE}: {key!r} must be a non-empty scalar')
    if top['source']['value']:
        fail(f'{PACKAGE}: `source` must be a mapping, not an inline value')
    if not top['source']['children']:
        fail(f'{PACKAGE}: `source` must declare the purl and an asset')

    if top['description']['value'] != cargo['description']:
        fail(
            'description must mirror Cargo.toml package.description: '
            f'{top["description"]["value"]!r} != {cargo["description"]!r}'
        )
    if top['homepage']['value'] != HOMEPAGE:
        fail(f'homepage must be {HOMEPAGE}, got {top["homepage"]["value"]!r}')

    licenses = list_values(top['licenses'], 'licenses')
    if not licenses:
        fail('licenses must contain at least one SPDX identifier')
    if licenses != [cargo['license']]:
        fail(
            'licenses must match Cargo.toml package.license: '
            f'{licenses!r} != {[cargo["license"]]!r}'
        )
    for license_id in licenses:
        if not re.fullmatch(r'[A-Za-z0-9.+-]+', license_id):
            fail(f'licenses: {license_id!r} is not an SPDX identifier')

    languages = list_values(top['languages'], 'languages')
    for language in languages:
        if not language.strip():
            fail('languages: empty list entry')

    categories = list_values(top['categories'], 'categories')
    if not categories:
        fail('categories must contain at least one category')
    for category in categories:
        if category not in CATEGORIES:
            fail(
                f'categories: {category!r} is not one of {", ".join(CATEGORIES)}'
            )
    if categories != ['LSP']:
        fail(f'categories must be exactly [LSP], got {categories!r}')

    source_fields, assets = parse_asset(top['source']['children'])
    purl = source_fields.get('id', '')
    if not purl:
        fail('source.id must be a non-empty purl')
    match = PURL_PATTERN.fullmatch(purl)
    if not match:
        fail(
            f'source.id must look like pkg:github/{OWNER_REPO}@v<version>, '
            f'got {purl!r}'
        )
    if f'{match["owner"]}/{match["repo"]}' != OWNER_REPO:
        fail(f'source.id owner/repo must be {OWNER_REPO}, got {purl!r}')
    if match['version'] != cargo['version']:
        fail(
            f'source.id version {match["version"]!r} must equal Cargo.toml '
            f'version {cargo["version"]!r}'
        )

    if not assets:
        fail('source.asset must declare at least one target')
    if len(assets) != 1:
        fail(f'source.asset must declare exactly the Linux target, got {len(assets)}')
    asset = assets[0]
    if asset.get('target') != TARGET:
        fail(
            f'source.asset[0].target must be {TARGET!r} (GNU-linked Linux), '
            f'got {asset.get("target")!r}'
        )
    filename = asset.get('file', '')
    asset_match = ASSET_PATTERN.fullmatch(filename)
    if not asset_match:
        fail(
            'source.asset[0].file must match the release archive '
            f'magit-lsp-v<version>-{TARGET_TRIPLE}.tar.gz, got {filename!r}'
        )
    expected_filename = (
        f'magit-lsp-v{cargo["version"]}-{TARGET_TRIPLE}.tar.gz'
    )
    if filename != expected_filename:
        fail(
            f'asset filename {filename!r} must match Cargo.toml version '
            f'{cargo["version"]!r} ({expected_filename!r})'
        )

    stem = filename[: -len('.tar.gz')]
    expected_bin = f'{stem}/magit-lsp'
    asset_bin = asset.get('bin', '')
    if asset_bin != expected_bin:
        fail(
            'source.asset[0].bin must point inside the archive\'s top-level '
            f'directory like golangci-lint\'s packages: {expected_bin!r}, '
            f'got {asset_bin!r}'
        )

    bins = mapping(top['bin'])
    if bins != {'magit-lsp': '{{source.asset.bin}}'}:
        fail(
            'top-level bin must be `magit-lsp: "{{source.asset.bin}}"`, '
            f'got {bins!r}'
        )

    print(f'mason: {PACKAGE} matches Cargo.toml {cargo["version"]}')
    print(f'  purl:  {purl}')
    print(f'  asset: {filename} ({TARGET})')
    print(f'  bin:   {asset_bin}')


def main():
    try:
        check()
    except CheckError as error:
        print(f'FAILED: mason: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
