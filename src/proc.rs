//! Starting a program boxvibe does not wait for, without leaving a zombie.

pub use vibekit::proc::detached;

#[cfg(test)]
mod tests;
