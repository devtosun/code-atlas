#![cfg_attr(feature = "nightly", allow(incomplete_features))]

use std::{collections::HashMap as Map, sync::Arc};
use crate::ports::{self, Repository as Repo};

pub mod ports {
    pub trait Repository<T> {
        async fn save(&self, value: T) -> Result<(), String>;
    }
}

#[cfg(feature = "billing")]
pub struct Invoice<T> {
    pub value: T,
}

pub enum Status<T> {
    Ready(T),
    Closed,
}

pub trait Summary<T> {
    fn summarize(&self, value: T) -> String;
}

impl<T: ToString> Invoice<T> {
    pub async fn total<U>(&self, extra: U) -> String
    where
        U: ToString,
    {
        let value = self.value.to_string();
        let format_value = |suffix: &str| external::format(value.clone(), suffix);
        format_value(&extra.to_string())
    }
}

impl<T: ToString> Summary<T> for Invoice<T> {
    fn summarize(&self, value: T) -> String {
        value.to_string()
    }
}

macro_rules! invoice_value {
    ($value:expr) => { Invoice { value: $value } };
}

pub async fn execute<R: Repo<String>>(repository: &R) -> Result<(), String> {
    let invoice = invoice_value!("İstanbul".to_owned());
    repository.save(invoice.value).await
}

pub fn özet() -> &'static str { "İstanbul" }

// phantom_call() is not executable source.
pub const NOTE: &str = "phantom_call()";
pub static FALLBACK: &str = "özet";
