// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded affix "><a href="index.html">Home</a></li><li class="chapter-item expanded affix "><li class="part-title">Design</li><li class="chapter-item expanded "><a href="design/overview.html"><strong aria-hidden="true">1.</strong> Product And Runtime Overview</a></li><li class="chapter-item expanded "><a href="design/architecture.html"><strong aria-hidden="true">2.</strong> Architecture</a></li><li class="chapter-item expanded "><a href="design/flows.html"><strong aria-hidden="true">3.</strong> Runtime Flows</a></li><li class="chapter-item expanded "><a href="design/contracts.html"><strong aria-hidden="true">4.</strong> Design Contracts</a></li><li class="chapter-item expanded affix "><li class="part-title">Rust Workspace</li><li class="chapter-item expanded "><a href="crates/protocol.html"><strong aria-hidden="true">5.</strong> Protocol Crate</a></li><li class="chapter-item expanded "><a href="crates/session-kernel.html"><strong aria-hidden="true">6.</strong> Session Kernel</a></li><li class="chapter-item expanded "><a href="crates/scheduler.html"><strong aria-hidden="true">7.</strong> Scheduler</a></li><li class="chapter-item expanded "><a href="crates/openai-rs.html"><strong aria-hidden="true">8.</strong> OpenAI Client</a></li><li class="chapter-item expanded "><a href="crates/status.html"><strong aria-hidden="true">9.</strong> Status Engine</a></li><li class="chapter-item expanded "><a href="crates/pave-router.html"><strong aria-hidden="true">10.</strong> PAVE Router</a></li><li class="chapter-item expanded "><a href="crates/persistence.html"><strong aria-hidden="true">11.</strong> Persistence Crates</a></li><li class="chapter-item expanded "><a href="crates/ui-bridge.html"><strong aria-hidden="true">12.</strong> UI Bridge</a></li><li class="chapter-item expanded affix "><li class="part-title">Runtime Entrypoints</li><li class="chapter-item expanded "><a href="runtime/entrypoints.html"><strong aria-hidden="true">13.</strong> Binary, CLI, Web, And VSCode Stdio</a></li><li class="chapter-item expanded "><a href="runtime/tools.html"><strong aria-hidden="true">14.</strong> Local Tool Gateway</a></li><li class="chapter-item expanded "><a href="runtime/skills-mcp.html"><strong aria-hidden="true">15.</strong> Skill And MCP Runtime</a></li><li class="chapter-item expanded affix "><li class="part-title">Applications</li><li class="chapter-item expanded "><a href="apps/vscode-extension.html"><strong aria-hidden="true">16.</strong> VSCode Extension</a></li><li class="chapter-item expanded affix "><li class="part-title">Operations</li><li class="chapter-item expanded "><a href="operations/configuration.html"><strong aria-hidden="true">17.</strong> Configuration And Data Locations</a></li><li class="chapter-item expanded "><a href="operations/build-test-docs.html"><strong aria-hidden="true">18.</strong> Build, Test, And Documentation</a></li><li class="chapter-item expanded affix "><li class="part-title">Appendix</li><li class="chapter-item expanded "><a href="appendix/module-index.html"><strong aria-hidden="true">19.</strong> Module Index</a></li><li class="chapter-item expanded "><a href="appendix/interface-reference.html"><strong aria-hidden="true">20.</strong> Interface Reference</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
