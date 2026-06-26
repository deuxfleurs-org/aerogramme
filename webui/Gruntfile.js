// Load Grunt
module.exports = function(grunt) {
  var js_files = {
    'public/js/Common.js': ['js/Common/*.app.js', 'js/Common/*.filter.js', 'js/Common/*Controller.js', 'js/Common/*.service.js', 'js/Common/*.directive.js', 'js/Common/utils.js'],
    'public/js/Main.js': ['js/Main/Main.app.js'],
    'public/js/Scheduler.services.js': ['js/Scheduler/*.service.js', 'js/Scheduler/*Controller.js', 'js/Scheduler/*.directive.js'],
    'public/js/Scheduler.js': ['js/Scheduler/Scheduler.app.js'],
    'public/js/Contacts.services.js': ['js/Contacts/*.service.js'],
    'public/js/Contacts.js': ['js/Contacts/Contacts.app.js', 'js/Contacts/*Controller.js', 'js/Contacts/*.directive.js'],
    'public/js/Mailer.services.js': ['js/Mailer/*.service.js', 'js/Mailer/*Controller.js', 'js/Mailer/*.directive.js'],
    'public/js/Mailer.js': ['js/Mailer/Mailer.app.js'],
    'public/js/Mailer.app.popup.js': ['js/Mailer/Mailer.popup.js'],
    'public/js/Preferences.services.js': ['js/Preferences/*.service.js'],
    'public/js/Preferences.js': ['js/Preferences/Preferences.app.js', 'js/Preferences/*Controller.js'],
    'public/js/Administration.services.js': ['js/Administration/*.service.js'],
    'public/js/Administration.js': ['js/Administration/Administration.app.js', 'js/Administration/*Controller.js']

  };
  var custom_vendor_files = {
    'public/js/vendor/angular-file-upload.min.js': ['node_modules/angular-file-upload/dist/angular-file-upload.js', 'js/Common/angular-file-upload.trump.js'],
    'public/js/vendor/FileSaver.min.js': ['node_modules/file-saver/dist/FileSaver.js', 'js/vendor/ckeditor/build/translations/*.js']
  };

  const sass = require('sass');
  require('time-grunt')(grunt);

  // Tasks
  grunt.initConfig({
    pkg: grunt.file.readJSON('package.json'),
    sass: {
      options: {
        implementation: sass,
        sourceMap: true,
        outFile: 'public/css/styles.css',
        noCache: true,
        includePaths: ['scss/',
                       'node_modules/breakpoint-sass/stylesheets/'
        ]
      },
      target: {
        files: {
          'public/css/styles.css': 'scss/styles.scss',
          'public/css/no-animation.css': 'scss/core/no-animation.scss'
        },
      },
    },
    postcss: {
      target: {
        options: {
          map: true,
          processors: [
            // See angular-material/gulp/util.js
            // See browserslist in package.json
            require('autoprefixer')()
          ]
        },
        src: ['public/css/styles.css', 'public/css/no-animation.css']
      }
    },
    cssmin: {
      options: {
        sourceMap: true,
      },
      target: {
        files: {
          'public/css/styles.css': 'public/css/styles.css',
          'public/css/no-animation.css': 'public/css/no-animation.css'
        }
      }
    },
    jshint: {
      files: [].concat(Object.keys(js_files).map(function(v) { return js_files[v]; }))
    },
    uglify: {
      options: {
        sourceMap: true
      },
      dist: {
        options: {
          compress: true,
          sourceMapIncludeSources: true
        },
        files: js_files
      },
      dev: {
        options: {
          compress: false,
          mangle: false,
        },
        files: js_files
      },
      vendor: {
        options: {
          compress: true,
        },
        files: custom_vendor_files,
      }
    },
    watch: {
      grunt: {
        files: ['Gruntfile.js']
      },
      sass: {
        files: 'scss/**/*.scss',
        tasks: ['sass']
      },
      js: {
        files: Object.keys(js_files).map(function(key) { return js_files[key]; }),
        tasks: ['js']
      }
    }
  });

  // Load Grunt plugins
  grunt.loadNpmTasks('grunt-sass');
  grunt.loadNpmTasks('grunt-postcss');
  grunt.loadNpmTasks('grunt-contrib-cssmin');
  grunt.loadNpmTasks('grunt-contrib-jshint');
  grunt.loadNpmTasks('grunt-contrib-uglify');
  grunt.loadNpmTasks('grunt-contrib-watch');

  // Register Grunt tasks
  grunt.task.registerTask('static', function() {
    var options = {
      'src': 'node_modules',
      'js_dest': 'public/js/vendor/',
      'fonts_dest': 'public/fonts/',
      'css_dest': 'public/css/'
    };
    grunt.log.subhead('Copying JavaScript files');
    var js = [
      '<%= src %>/angular/angular{,.min}.js{,.map}',
      '<%= src %>/angular-animate/angular-animate{,.min}.js{,.map}',
      '<%= src %>/angular-sanitize/angular-sanitize{,.min}.js{,.map}',
      '<%= src %>/angular-aria/angular-aria{,.min}.js{,.map}',
      '<%= src %>/angular-cookies/angular-cookies{,.min}.js{,.map}',
      '<%= src %>/angular-messages/angular-messages{,.min}.js{,.map}',
      '<%= src %>/angular-material/angular-material{,.min}.js',
      '<%= src %>/angular-ui-router/release/angular-ui-router{,.min}.js{,.map}',
      //'<%= src %>/ng-file-upload/ng-file-upload{,.min}.js{,map}',
      '<%= src %>/ng-sortable/dist/ng-sortable.min.js{,map}',
      '<%= src %>/lodash/lodash{,.min}.js',
      '<%= src %>/qrcodejs/qrcode{,.min}.js',
      '<%= src %>/punycode/punycode.js',
      '<%= src %>/mark.js/dist/mark.min.js'
    ];
    for (var j = 0; j < js.length; j++) {
      var files = grunt.file.expand(grunt.template.process(js[j], {data: options}));
      for (var i = 0; i < files.length; i++) {
        var src = files[i];
        var paths = src.split('/');
        var dest = options.js_dest + paths[paths.length - 1];
        grunt.file.copy(src, dest);
        grunt.log.ok("copy " + src + " => " + dest);
        // Patch for module.exports for puny code
        if (dest.indexOf('punycode') > 0) {
          var fs = require('fs');
          var fileContent = fs.readFileSync(dest, { encoding: 'utf8', flag: 'r' });
          fileContent = fileContent.replace("module.exports", "//module.exports");
          fs.writeFileSync(dest, fileContent);
        }
      }
    }
    grunt.log.subhead('Copying font files');
    var fonts = [
	"fonts/FiraMono-Bold.eot",
	"fonts/FiraMono-Bold.ttf",
	"fonts/FiraMono-Bold.woff",
	"fonts/FiraMono-Medium.eot",
	"fonts/FiraMono-Medium.ttf",
	"fonts/FiraMono-Medium.woff",
	"fonts/FiraMono-Regular.eot",
	"fonts/FiraMono-Regular.ttf",
	"fonts/FiraMono-Regular.woff",
	"fonts/FiraSans-Bold.eot",
	"fonts/FiraSans-BoldItalic.eot",
	"fonts/FiraSans-BoldItalic.ttf",
	"fonts/FiraSans-BoldItalic.woff",
	"fonts/FiraSans-Bold.ttf",
	"fonts/FiraSans-Bold.woff",
	"fonts/FiraSans-Book.eot",
	"fonts/FiraSans-BookItalic.eot",
	"fonts/FiraSans-BookItalic.ttf",
	"fonts/FiraSans-BookItalic.woff",
	"fonts/FiraSans-Book.ttf",
	"fonts/FiraSans-Book.woff",
	"fonts/FiraSans-Eight.eot",
	"fonts/FiraSans-EightItalic.eot",
	"fonts/FiraSans-EightItalic.ttf",
	"fonts/FiraSans-EightItalic.woff",
	"fonts/FiraSans-Eight.ttf",
	"fonts/FiraSans-Eight.woff",
	"fonts/FiraSans-ExtraBold.eot",
	"fonts/FiraSans-ExtraBoldItalic.eot",
	"fonts/FiraSans-ExtraBoldItalic.ttf",
	"fonts/FiraSans-ExtraBoldItalic.woff",
	"fonts/FiraSans-ExtraBold.ttf",
	"fonts/FiraSans-ExtraBold.woff",
	"fonts/FiraSans-ExtraLight.eot",
	"fonts/FiraSans-ExtraLightItalic.eot",
	"fonts/FiraSans-ExtraLightItalic.ttf",
	"fonts/FiraSans-ExtraLightItalic.woff",
	"fonts/FiraSans-ExtraLight.ttf",
	"fonts/FiraSans-ExtraLight.woff",
	"fonts/FiraSans-Four.eot",
	"fonts/FiraSans-FourItalic.eot",
	"fonts/FiraSans-FourItalic.ttf",
	"fonts/FiraSans-FourItalic.woff",
	"fonts/FiraSans-Four.ttf",
	"fonts/FiraSans-Four.woff",
	"fonts/FiraSans-Hair.eot",
	"fonts/FiraSans-HairItalic.eot",
	"fonts/FiraSans-HairItalic.ttf",
	"fonts/FiraSans-HairItalic.woff",
	"fonts/FiraSans-Hair.ttf",
	"fonts/FiraSans-Hair.woff",
	"fonts/FiraSans-Heavy.eot",
	"fonts/FiraSans-HeavyItalic.eot",
	"fonts/FiraSans-HeavyItalic.ttf",
	"fonts/FiraSans-HeavyItalic.woff",
	"fonts/FiraSans-Heavy.ttf",
	"fonts/FiraSans-Heavy.woff",
	"fonts/FiraSans-Italic.eot",
	"fonts/FiraSans-Italic.ttf",
	"fonts/FiraSans-Italic.woff",
	"fonts/FiraSans-Light.eot",
	"fonts/FiraSans-LightItalic.eot",
	"fonts/FiraSans-LightItalic.ttf",
	"fonts/FiraSans-LightItalic.woff",
	"fonts/FiraSans-Light.ttf",
	"fonts/FiraSans-Light.woff",
	"fonts/FiraSans-Medium.eot",
	"fonts/FiraSans-MediumItalic.eot",
	"fonts/FiraSans-MediumItalic.ttf",
	"fonts/FiraSans-MediumItalic.woff",
	"fonts/FiraSans-Medium.ttf",
	"fonts/FiraSans-Medium.woff",
	"fonts/FiraSans-Regular.eot",
	"fonts/FiraSans-Regular.ttf",
	"fonts/FiraSans-Regular.woff",
	"fonts/FiraSans-SemiBold.eot",
	"fonts/FiraSans-SemiBoldItalic.eot",
	"fonts/FiraSans-SemiBoldItalic.ttf",
	"fonts/FiraSans-SemiBoldItalic.woff",
	"fonts/FiraSans-SemiBold.ttf",
	"fonts/FiraSans-SemiBold.woff",
	"fonts/FiraSans-Thin.eot",
	"fonts/FiraSans-ThinItalic.eot",
	"fonts/FiraSans-ThinItalic.ttf",
	"fonts/FiraSans-ThinItalic.woff",
	"fonts/FiraSans-Thin.ttf",
	"fonts/FiraSans-Thin.woff",
	"fonts/FiraSans-Two.eot",
	"fonts/FiraSans-TwoItalic.eot",
	"fonts/FiraSans-TwoItalic.ttf",
	"fonts/FiraSans-TwoItalic.woff",
	"fonts/FiraSans-Two.ttf",
	"fonts/FiraSans-Two.woff",
	"fonts/FiraSans-Ultra.eot",
	"fonts/FiraSans-UltraItalic.eot",
	"fonts/FiraSans-UltraItalic.ttf",
	"fonts/FiraSans-UltraItalic.woff",
	"fonts/FiraSans-UltraLight.eot",
	"fonts/FiraSans-UltraLightItalic.eot",
	"fonts/FiraSans-UltraLightItalic.ttf",
	"fonts/FiraSans-UltraLightItalic.woff",
	"fonts/FiraSans-UltraLight.ttf",
	"fonts/FiraSans-UltraLight.woff",
	"fonts/FiraSans-Ultra.ttf",
	"fonts/FiraSans-Ultra.woff",
	"fonts/MaterialIcons-Regular.eot",
	"fonts/MaterialIcons-Regular.ttf",
	"fonts/MaterialIcons-Regular.woff",
	"fonts/MaterialIcons-Regular.woff2",
    ];
    for (var j = 0; j < fonts.length; j++) {
      var files = grunt.file.expand(grunt.template.process(fonts[j], {data: options}));
      for (var i = 0; i < files.length; i++) {
        var src = files[i];
        var paths = src.split('/');
        var dest = options.fonts_dest + paths[paths.length - 1];
        grunt.file.copy(src, dest);
        grunt.log.ok("copy " + src + " => " + dest);
      }
    }
    grunt.log.subhead('Copying CSS files');
    var css = [
	"css/icons.css",
	"css/theme-default.css",
    ];
    for (var j = 0; j < css.length; j++) {
      var files = grunt.file.expand(grunt.template.process(css[j], {data: options}));
      for (var i = 0; i < files.length; i++) {
        var src = files[i];
        var paths = src.split('/');
        var dest = options.css_dest + paths[paths.length - 1];
        grunt.file.copy(src, dest);
        grunt.log.ok("copy " + src + " => " + dest);
      }
    }
    grunt.task.run('uglify:vendor');
  });
  grunt.task.registerTask('build', ['static', 'uglify:dist', 'sass', 'postcss', 'cssmin']);
  // Tasks for developers
  grunt.task.registerTask('default', ['watch']);
  grunt.task.registerTask('css', ['sass', 'postcss']);
  grunt.task.registerTask('js', ['jshint', 'uglify:dev']);
  grunt.task.registerTask('dev', ['css', 'js']);
};
