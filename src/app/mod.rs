#![doc = include_str!("./README.md")]
//! Leptos frontend application (compiled to WASM via trunk).

pub(crate) mod components;
pub(crate) mod fetch;
pub(crate) mod models;
pub(crate) mod pages;
pub(crate) mod utils;

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;
use lucide_leptos::{Book, Cog as SettingsIcon, FlaskConical, Github, LayoutDashboard, Package};

use pages::*;
use utils::SidebarContext;

#[component]
pub fn App() -> impl IntoView {
    let sidebar_content = RwSignal::new_local(None);
    provide_context(SidebarContext(sidebar_content));

    view! {
        <Router>
            <div class="flex h-screen bg-slate-950 text-slate-100 font-sans">
                // Sidebar
                <nav class="w-64 border-r border-slate-800 flex flex-col p-4 bg-slate-900/50">
                    <div class="flex items-center space-x-3 px-2 py-6 mb-6">
                        <div class="p-2 bg-blue-600 rounded-lg shadow-lg shadow-blue-900/20">
                            <Package size=24 />
                        </div>
                        <span class="text-2xl font-bold tracking-tight text-white">"ExpMan"</span>
                    </div>

                    <div class="space-y-1">
                        <A href="/" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <LayoutDashboard size=20 />
                            </div>
                            <span class="font-medium">"Dashboard"</span>
                        </A>

                        <A href="/experiments" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <FlaskConical size=20 />
                            </div>
                            <span class="font-medium">"Experiments"</span>
                        </A>

                        <div class="pt-4 mt-4 border-t border-slate-800 empty:hidden">
                             {move || sidebar_content.get().map(|f| f())}
                        </div>
                    </div>

                    <div class="mt-auto space-y-1">
                        <A href="/settings" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <SettingsIcon size=20 />
                            </div>
                            <span class="font-medium">"Settings"</span>
                        </A>

                        <a href="https://lokeshmohanty.github.io/expman-rs" target="_blank" rel="noopener noreferrer" class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <Book size=20 />
                            </div>
                            <span class="font-medium">"Documentation"</span>
                        </a>

                        <a href="https://github.com/lokeshmohanty/expman-rs" target="_blank" rel="noopener noreferrer" class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <Github size=20 />
                            </div>
                            <span class="font-medium">"GitHub"</span>
                        </a>
                    </div>
                </nav>

                // Main Content
                <main class="flex-grow overflow-auto p-8">
                    <Routes fallback=|| view! { <NotFound /> }.into_any()>
                        <Route path=path!("/") view=|| view! { <Dashboard /> } />
                        <Route path=path!("/experiments") view=|| view! { <Experiments /> } />
                        <Route path=path!("/experiments/:id") view=|| view! { <ExperimentDetail /> } />
                        <Route path=path!("/settings") view=|| view! { <SettingsPage /> } />
                    </Routes>
                </main>
            </div>
        </Router>
    }.into_any()
}
