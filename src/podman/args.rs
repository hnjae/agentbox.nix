// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

#[derive(Debug, Default)]
pub(super) struct PodmanArgs {
    values: Vec<String>,
}

impl PodmanArgs {
    pub(super) fn from<const N: usize>(values: [&str; N]) -> Self {
        let mut args = Self::default();
        args.extend(values);
        args
    }

    pub(super) fn flag(&mut self, value: impl Into<String>) {
        self.values.push(value.into());
    }

    pub(super) fn option(&mut self, name: &'static str, value: impl Into<String>) {
        self.flag(name);
        self.flag(value);
    }

    pub(super) fn key_value_option(&mut self, name: &'static str, key: &str, value: &str) {
        self.option(name, format!("{key}={value}"));
    }

    pub(super) fn extend<I, S>(&mut self, values: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.values.extend(values.into_iter().map(Into::into));
    }

    pub(super) fn into_vec(self) -> Vec<String> {
        self.values
    }
}

#[cfg(test)]
pub(super) fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_string).collect()
}
