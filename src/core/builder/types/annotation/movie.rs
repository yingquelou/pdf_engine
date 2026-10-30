//! Movie 注解专有条目（12.5.6.17 Table 186）的构建态与构建期校验。
//!
//! `/Movie` 是电影字典（13.4 Table 295），Movie 注解必填，写成内联字典；
//! `/A` 在该子类型下表示注解被激活时的播放设置（布尔或电影激活字典
//! Table 296），与通用注解动作 `/A` 互斥。`/T` 标题复用
//! [`PdfAnnotation::with_title`]，与 Table 186 的 `/T` 同义。
//!
//! 13.4 声明电影特性已被 13.2 的多媒体框架取代（obsolescent）；本模块
//! 只做语法层建模，不写电影文件内容，也不支持海报图像流形态。

use super::{AnnotationKind, Movie, MoviePlayback, PdfAnnotation};

impl PdfAnnotation {
    /// 电影字典 `/Movie`（Table 186，Movie 注解必填）。
    pub fn with_movie(mut self, movie: Movie) -> Self {
        self.movie = Some(movie);
        self
    }

    /// 电影播放设置 `/A`（布尔或电影激活字典；缺省 `true`）。
    pub fn with_movie_activation(mut self, playback: MoviePlayback) -> Self {
        self.movie_playback = Some(playback);
        self
    }

    /// 电影字典（Movie 注解）。
    pub(crate) fn movie(&self) -> Option<&Movie> {
        self.movie.as_ref()
    }

    /// 电影播放设置（Movie 注解 `/A`）。
    pub(crate) fn movie_playback(&self) -> Option<&MoviePlayback> {
        self.movie_playback.as_ref()
    }

    /// 校验 `/Movie` 与 `/A` 的子类型归属、必填性与互斥关系。
    pub(super) fn validate_movie_entries(&self) -> Result<(), String> {
        if self.kind == AnnotationKind::Movie {
            if self.movie.is_none() {
                return Err("Movie annotation requires /Movie (movie dictionary)".into());
            }
            if self.action.is_some() {
                return Err(
                    "Movie annotation /A holds playback settings and cannot carry an action".into(),
                );
            }
            if self.destination.is_some() {
                return Err(
                    "Movie annotation /Dest is not permitted (Table 186 has no /Dest)".into(),
                );
            }
        } else {
            if self.movie.is_some() {
                return Err(format!(
                    "annotation /Movie is specific to Movie annotations, got {}",
                    self.kind.name()
                ));
            }
            if self.movie_playback.is_some() {
                return Err(format!(
                    "annotation /A playback settings are specific to Movie annotations, got {}",
                    self.kind.name()
                ));
            }
        }
        if let Some(movie) = &self.movie {
            movie.validate()?;
        }
        if let Some(playback) = &self.movie_playback {
            playback.validate()?;
        }
        Ok(())
    }
}
