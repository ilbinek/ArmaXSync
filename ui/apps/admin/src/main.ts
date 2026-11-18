import { mount } from "svelte";
import "@axs/kit/styles.css";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
