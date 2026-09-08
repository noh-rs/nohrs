import { parse as parseYaml } from 'yaml'

/**
 * Turns the YAML header that `remark-frontmatter` parked in the tree into
 * `export const frontmatter = {...}` on the compiled MDX module, so a route can
 * read a post's title without also loading and re-parsing the raw file.
 *
 * This replaces `remark-mdx-frontmatter`, which reaches the same result but
 * pulls in `toml`, a package with two unfixed high-severity advisories.
 */
export default function remarkFrontmatterExport() {
  return (tree) => {
    const index = tree.children.findIndex((node) => node.type === 'yaml')
    const data = index === -1 ? {} : (parseYaml(tree.children[index].value) ?? {})
    if (index !== -1) tree.children.splice(index, 1)

    tree.children.unshift({
      type: 'mdxjsEsm',
      value: '',
      data: {
        estree: {
          type: 'Program',
          sourceType: 'module',
          body: [
            {
              type: 'ExportNamedDeclaration',
              specifiers: [],
              source: null,
              declaration: {
                type: 'VariableDeclaration',
                kind: 'const',
                declarations: [
                  {
                    type: 'VariableDeclarator',
                    id: { type: 'Identifier', name: 'frontmatter' },
                    init: valueToEstree(data),
                  },
                ],
              },
            },
          ],
        },
      },
    })
  }
}

function valueToEstree(value) {
  if (value === null || value === undefined) return { type: 'Literal', value: null, raw: 'null' }
  if (value instanceof Date) return { type: 'Literal', value: value.toISOString(), raw: JSON.stringify(value.toISOString()) }
  if (Array.isArray(value)) {
    return { type: 'ArrayExpression', elements: value.map(valueToEstree) }
  }
  if (typeof value === 'object') {
    return {
      type: 'ObjectExpression',
      properties: Object.entries(value).map(([key, item]) => ({
        type: 'Property',
        kind: 'init',
        method: false,
        shorthand: false,
        computed: false,
        key: { type: 'Literal', value: key, raw: JSON.stringify(key) },
        value: valueToEstree(item),
      })),
    }
  }
  return { type: 'Literal', value, raw: JSON.stringify(value) }
}
