// AiOS Releases — Fetch and render GitHub Releases
// Uses safe DOM methods only (no innerHTML with untrusted content)

(function () {
  "use strict";

  var REPO = "swit-work/ai-os";
  var API_URL = "https://api.github.com/repos/" + REPO + "/releases";
  var container = document.getElementById("releases-list");

  function formatBytes(bytes) {
    if (bytes === 0) return "0 B";
    var units = ["B", "KB", "MB", "GB"];
    var i = Math.floor(Math.log(bytes) / Math.log(1024));
    var value = bytes / Math.pow(1024, i);
    return value.toFixed(i > 0 ? 1 : 0) + " " + units[i];
  }

  function formatDate(dateStr) {
    var d = new Date(dateStr);
    return d.toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  }

  // Strip markdown to plain text for safe rendering
  function stripMarkdown(md) {
    if (!md) return "";
    return md
      .replace(/#{1,6}\s+/g, "")       // headers
      .replace(/\*\*(.+?)\*\*/g, "$1")  // bold
      .replace(/\*(.+?)\*/g, "$1")      // italic
      .replace(/_(.+?)_/g, "$1")        // italic alt
      .replace(/`(.+?)`/g, "$1")        // inline code
      .replace(/\[(.+?)\]\(.+?\)/g, "$1") // links → text only
      .replace(/!\[.*?\]\(.+?\)/g, "")  // images → remove
      .replace(/^[-*+]\s+/gm, "- ")     // normalize list bullets
      .replace(/^\d+\.\s+/gm, "- ")     // ordered lists → bullets
      .replace(/^>\s+/gm, "")           // blockquotes
      .replace(/---+/g, "")             // horizontal rules
      .trim();
  }

  function renderRelease(release, isFirst) {
    var card = document.createElement("div");
    card.className = "release-card";

    // Header: version + date + badges
    var header = document.createElement("div");
    header.className = "release-header";

    var version = document.createElement("span");
    version.className = "release-version";
    version.textContent = release.tag_name;
    header.appendChild(version);

    var date = document.createElement("span");
    date.className = "release-date";
    date.textContent = formatDate(release.published_at || release.created_at);
    header.appendChild(date);

    if (release.prerelease) {
      var preBadge = document.createElement("span");
      preBadge.className = "release-badge pre-release";
      preBadge.textContent = "Pre-release";
      header.appendChild(preBadge);
    } else if (isFirst) {
      var latestBadge = document.createElement("span");
      latestBadge.className = "release-badge latest";
      latestBadge.textContent = "Latest";
      header.appendChild(latestBadge);
    }

    card.appendChild(header);

    // Release notes (plain text, safe)
    if (release.body) {
      var notes = document.createElement("div");
      notes.className = "release-notes";
      notes.textContent = stripMarkdown(release.body);
      card.appendChild(notes);
    }

    // Download assets
    if (release.assets && release.assets.length > 0) {
      var assets = document.createElement("div");
      assets.className = "release-assets";

      release.assets.forEach(function (asset) {
        var link = document.createElement("a");
        link.className = "release-asset";
        link.href = asset.browser_download_url;
        link.rel = "noopener";

        var nameSpan = document.createElement("span");
        nameSpan.textContent = asset.name;
        link.appendChild(nameSpan);

        var sizeSpan = document.createElement("span");
        sizeSpan.className = "release-asset-size";
        sizeSpan.textContent = "(" + formatBytes(asset.size) + ")";
        link.appendChild(sizeSpan);

        assets.appendChild(link);
      });

      card.appendChild(assets);
    }

    return card;
  }

  function renderEmpty() {
    // Clear container
    while (container.firstChild) {
      container.removeChild(container.firstChild);
    }

    var empty = document.createElement("div");
    empty.className = "release-empty";

    var msg = document.createElement("p");
    msg.textContent = "No releases yet. The first release is coming soon.";
    empty.appendChild(msg);

    var link = document.createElement("a");
    link.href = "https://github.com/" + REPO + "/releases";
    link.target = "_blank";
    link.rel = "noopener";
    link.className = "btn btn-secondary";
    link.textContent = "Check GitHub Releases";
    empty.appendChild(link);

    container.appendChild(empty);
  }

  function renderError(message) {
    while (container.firstChild) {
      container.removeChild(container.firstChild);
    }

    var empty = document.createElement("div");
    empty.className = "release-empty";

    var msg = document.createElement("p");
    msg.textContent = message;
    empty.appendChild(msg);

    var link = document.createElement("a");
    link.href = "https://github.com/" + REPO + "/releases";
    link.target = "_blank";
    link.rel = "noopener";
    link.className = "btn btn-secondary";
    link.textContent = "View on GitHub";
    empty.appendChild(link);

    container.appendChild(empty);
  }

  function renderReleases(releases) {
    while (container.firstChild) {
      container.removeChild(container.firstChild);
    }

    if (!releases || releases.length === 0) {
      renderEmpty();
      return;
    }

    var firstNonPrerelease = true;
    releases.forEach(function (release) {
      var isLatest = false;
      if (!release.prerelease && firstNonPrerelease) {
        isLatest = true;
        firstNonPrerelease = false;
      }
      container.appendChild(renderRelease(release, isLatest));
    });
  }

  function fetchReleases() {
    fetch(API_URL, {
      headers: { Accept: "application/vnd.github.v3+json" },
    })
      .then(function (response) {
        if (!response.ok) {
          throw new Error("GitHub API returned " + response.status);
        }
        return response.json();
      })
      .then(function (data) {
        renderReleases(data);
      })
      .catch(function (err) {
        console.error("Failed to fetch releases:", err);
        renderError(
          "Could not load releases. The GitHub API may be temporarily unavailable."
        );
      });
  }

  // Start loading
  fetchReleases();
})();
