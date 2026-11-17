// SPDX-License-Identifier: AGPL-3.0-or-later

import js from '@eslint/js'
import svelte from 'eslint-plugin-svelte'
import globals from 'globals'
import ts from 'typescript-eslint'

export default ts.config(
  {
    ignores: ['**/dist/', '**/target/', 'app/gen/', '.*/'],
  },
  js.configs.recommended,
  ...ts.configs.strict,
  ...svelte.configs.recommended,
  {
    languageOptions: { globals: globals.browser },
    rules: {
      // the dialog plugin makes these async, a check like `if (confirm(...))`
      // then always passes: Baguette draws its own (lib/dialogs.svelte.ts)
      'no-restricted-globals': [
        'error',
        ...['confirm', 'prompt', 'alert'].map((name) => ({
          name,
          message: 'Use ask or tell from lib/dialogs.svelte.ts.',
        })),
      ],
    },
  },
  {
    files: ['**/*.config.{js,ts}'],
    languageOptions: { globals: globals.node },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser } },
  },
)
