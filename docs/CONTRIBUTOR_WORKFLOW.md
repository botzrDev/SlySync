# SlySync Contributor Workflow Guide

This document describes the correct workflow for contributing to the SlySync project, especially when working with protected branches and required commit signing. All contributors (including admins) are encouraged to follow these steps for consistency, security, and auditability.

---

## 1. Branching and Pull Requests (PRs)

- **Never push directly to protected branches** (e.g., `main`, `alpha`).
- **Always create a feature branch** for your work:
  ```sh
  git checkout -b my-feature-branch
  ```
- **Make your changes and commit** (see commit signing below).
- **Push your branch to the remote:**
  ```sh
  git push origin my-feature-branch
  ```
- **Open a Pull Request (PR)** on GitHub from your branch into the target branch (e.g., `alpha`).
- **Wait for code review and CI checks** to pass, then merge via the GitHub UI.

---

## 2. Commit Signing

- **Why:** Commit signing ensures authenticity and traceability.
- **How:**
  - **GPG:**
    1. Generate a GPG key:
       ```sh
       gpg --full-generate-key
       ```
    2. Add your GPG public key to your GitHub account.
    3. Configure git to use your key:
       ```sh
       git config --global user.signingkey <your-key-id>
       git config --global commit.gpgsign true
       ```
    4. Commit with signing:
       ```sh
       git commit -S -m "Your message"
       ```
  - **SSH (if supported):**
    1. Generate an SSH key if needed:
       ```sh
       ssh-keygen -t ed25519 -C "your_email@example.com"
       ```
    2. Add your SSH key to GitHub.
    3. Enable SSH commit signing in your git config.

---

## 3. Checklist for Every Contribution

- [ ] Work on a feature branch (not directly on `main` or `alpha`).
- [ ] Sign all your commits.
- [ ] Push your branch and open a PR.
- [ ] Wait for review and CI checks.
- [ ] Merge via the GitHub UI.

---

## 4. Admins and Rule Bypass

Admins can technically bypass branch protection rules, but **should only do so in emergencies or with a clear reason**. If you must bypass:
- Document the reason in the commit message or PR description.
- Consider following up with a PR for transparency.

---

## 5. Troubleshooting

- **Push rejected?**
  - Check if you are pushing directly to a protected branch.
  - Make sure your commits are signed.
  - Open a PR instead of pushing directly.
- **Signature not verified?**
  - Ensure your GPG/SSH key is added to GitHub and configured in git.
  - Use `git log --show-signature` to check commit signatures.

---

## 6. Resources

- [GitHub: About protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/about-protected-branches)
- [GitHub: Signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits)
- [Git: GPG commit signing](https://git-scm.com/book/en/v2/Git-Tools-Signing-Your-Work)

---

*Following this workflow helps keep SlySync secure, auditable, and collaborative for everyone!*
