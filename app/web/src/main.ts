// SPDX-License-Identifier: AGPL-3.0-or-later

import { mount } from 'svelte'
import App from './App.svelte'
import { app } from './lib/app.svelte'
import { listenToRust } from './lib/events'
import './styles/app.css'

listenToRust()
mount(App, { target: document.getElementById('app') ?? document.body })
void app.start()
