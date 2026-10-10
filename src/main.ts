import { mount } from 'svelte'
import App from './app/App.svelte'
import { settings } from './app/settings.svelte'
import { workspace } from './app/workspace.svelte'
import { cloud } from './features/cloud/cloud.svelte'
import './styles/app.css'

settings.init()
const app = mount(App, { target: document.getElementById('app')! })
void workspace.init()
void cloud.init()

export default app
