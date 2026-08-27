(function () {
    const text = (id, value) => {
        const element = document.getElementById(id);
        if (element) element.textContent = value || "";
    };

    const renderIdentity = (session) => {
        const item = document.getElementById("account-item");
        if (!item) return;

        text("account-avatar", (session.username || "?").slice(0, 1).toUpperCase());
        text("account-username", session.username || "-");
        text("account-role", session.role || "-");
        text("account-detail-username", session.username || "-");
        text("account-detail-role", session.role || "-");
        text("account-auth-mode", session.mode === "trusted-proxy" ? "Trusted proxy" : "Local");
        text("account-email", session.email || "");

        const subjectRow = document.getElementById("account-subject-row");
        if (subjectRow && session.subject) {
            text("account-subject", session.subject);
            subjectRow.style.display = "";
        }

        const groupsRow = document.getElementById("account-groups-row");
        const groups = document.getElementById("account-groups");
        if (groupsRow && groups && Array.isArray(session.groups) && session.groups.length > 0) {
            groups.replaceChildren(...session.groups.map((group) => {
                const code = document.createElement("code");
                code.className = "me-1";
                code.textContent = group;
                return code;
            }));
            groupsRow.style.display = "";
        }

        const headersSection = document.getElementById("account-headers-section");
        const headers = document.getElementById("account-headers");
        if (headersSection && headers && Array.isArray(session.headers) && session.headers.length > 0) {
            headers.replaceChildren(...session.headers.map((header) => {
                const row = document.createElement("div");
                row.className = "dropdown-item-text";
                const name = document.createElement("div");
                name.className = "text-secondary small font-monospace";
                name.textContent = header.name;
                const value = document.createElement("div");
                value.className = "small font-monospace text-break";
                value.textContent = header.value;
                row.append(name, value);
                return row;
            }));
            headersSection.style.display = "";
        }

        item.style.display = "";
        document.querySelectorAll(".admin-only").forEach((element) => {
            element.style.display = session.role === "Admin" ? "" : "none";
        });
    };

    document.addEventListener("DOMContentLoaded", async () => {
        const toggle = document.getElementById("account-toggle");
        const menu = document.getElementById("account-menu");
        if (toggle && menu) {
            toggle.addEventListener("click", (event) => {
                event.preventDefault();
                menu.style.display = menu.style.display === "block" ? "none" : "block";
                const themeMenu = document.getElementById("theme-menu");
                if (themeMenu) themeMenu.style.display = "none";
            });
            document.addEventListener("click", (event) => {
                if (!toggle.contains(event.target) && !menu.contains(event.target)) {
                    menu.style.display = "none";
                }
            });
        }

        try {
            const response = await fetch(window.__AUTH_SESSION_URL__, {
                headers: { Accept: "application/json" },
                credentials: "same-origin",
            });
            if (response.ok) renderIdentity(await response.json());
        } catch (_) {
            // The page remains usable; the proxy still enforces authorization.
        }
    });
})();
